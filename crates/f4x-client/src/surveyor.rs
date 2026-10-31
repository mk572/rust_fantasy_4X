//! The Surveyor: `Surveyor_Screen` and `Surveyor_Window_Display` in `Surveyor.c`.
//! F1 puts the side panel over the right-hand column; the square under the mouse is described:
//! its terrain, what lies on it, and what a city founded there would get from the land around
//! it, or why none can be founded. A click on the map centres it there, and Cancel, C or Escape
//! (or a right-click) leave.

use crate::font::Style;
use crate::input::Input;
use f4x_core::city::FoundCityError;
use f4x_core::feature::def::FeatureKind;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::text::{Args, Texts};
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, HUMAN};

const CANCEL_AT: (f32, f32) = (263.0, 181.0);

/// What the panel says about one square.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct SquareText {
    /// Terrain, and the two lines of what it gives a city.
    pub terrain: String,
    pub effects: [String; 2],
    /// What lies on it: a mineral, a site, a node or a city, with up to three lines.
    pub special: Vec<String>,
    /// A line of small text under the special's name.
    pub detail: String,
    /// What a city there would get: size, production and gold percent.
    pub resources: Option<(u32, i32, i32)>,
    /// Why no city can be founded.
    pub blocked: Option<String>,
}

/// Half-food units as the original writes them: "1/2 food", "1 1/2 food", "2 food".
pub(crate) fn food_text(texts: &Texts, half_units: u8) -> String {
    let whole = Args::new().count("n", half_units / 2);
    match (half_units / 2, half_units % 2) {
        (0, 0) => String::new(),
        (0, _) => texts.get("surveyor.food_half", &whole),
        (_, 0) => texts.get("surveyor.food", &whole),
        (_, _) => texts.get("surveyor.food_and_half", &whole),
    }
}

impl App {
    /// F1: the Surveyor takes over the side column.
    pub(crate) fn start_surveyor(&mut self) {
        self.road_plan = None;
        self.surveyor = true;
    }

    pub(crate) fn stop_surveyor(&mut self) {
        self.surveyor = false;
    }

    /// What the panel says about `pos`, or `None` for land the player has not explored.
    pub(crate) fn describe_square(&self, pos: Position) -> Option<SquareText> {
        if !self.game.is_explored(HUMAN, pos) {
            return None;
        }
        let map = self.game.map()?;
        let grid = map.plane(pos.plane);
        let content = self.game.content();
        let def = grid
            .get(pos.x as i32, pos.y as i32)
            .and_then(|t| content.terrains.get(t))?;
        let spoiled = grid.is_corrupted(pos.x as i32, pos.y as i32);
        let mut text = SquareText {
            terrain: def.name.clone(),
            ..Default::default()
        };
        let survey = self.game.site_survey(HUMAN, pos);
        let food = self
            .game
            .economy()
            .square_food(grid, pos.x as i32, pos.y as i32);
        text.effects[0] = food_text(self.texts(), food.clamp(0, 255) as u8);
        let production =
            self.game
                .economy()
                .square_production(grid, pos.x as i32, pos.y as i32, false);
        if production != 0 {
            text.effects[1] =
                self.text_with("surveyor.production", &Args::new().count("n", production));
        }
        if grid.has_river(pos.x as i32, pos.y as i32) && survey.gold_percent >= 20 {
            let gold = self.text_with(
                "surveyor.gold_percent",
                &Args::new().count("n", survey.gold_percent),
            );
            if text.effects[1].is_empty() {
                text.effects[1] = gold;
            } else {
                text.effects[0] = self.text_with(
                    "surveyor.food_and_gold",
                    &Args::new()
                        .text("food", &text.effects[0])
                        .text("gold", gold),
                );
            }
        } else if survey.gold_percent > 0 && def.class != TerrainClass::Ocean {
            text.effects[1] = self.text_with(
                "surveyor.gold_percent",
                &Args::new().count("n", survey.gold_percent),
            );
        }
        if spoiled {
            text.effects = [self.text("surveyor.corruption"), String::new()];
        }

        let feature = self.game.economy().feature_at(pos);
        let city = self.game.economy().city_at(pos);
        if let Some(city) = city {
            let size = self.city_size_name(city);
            text.special = vec![
                self.text_with("surveyor.size_of", &Args::new().text("size", size)),
                self.city_name(city),
            ];
        } else if let Some(f) = feature {
            text.special.push(f.name.clone());
            let b = f.city;
            let mut bits = Vec::new();
            if b.gold > 0 {
                bits.push(self.text_with("surveyor.bonus_gold", &Args::new().count("n", b.gold)));
            }
            if b.mana > 0 {
                bits.push(self.text_with("surveyor.bonus_power", &Args::new().count("n", b.mana)));
            }
            if b.food > 0 {
                bits.push(self.text_with("surveyor.bonus_food", &Args::new().count("n", b.food)));
            }
            if b.unit_cost_percent > 0 {
                bits.push(self.text_with(
                    "surveyor.unit_cost",
                    &Args::new().count("n", b.unit_cost_percent),
                ));
            }
            text.detail = self.texts().join(&bits);
            match f.kind {
                FeatureKind::Node => {
                    if let Some(n) = self.game.economy().node_at(pos) {
                        let realm = n.realm.clone();
                        let realm = realm[..1].to_uppercase() + &realm[1..];
                        text.special = vec![
                            self.text_with("surveyor.node", &Args::new().text("realm", realm))
                        ];
                        text.special.push(if n.owner.is_some() {
                            self.text(if n.warped {
                                "surveyor.warped"
                            } else if n.guardian {
                                "surveyor.guardian_spirit"
                            } else {
                                "surveyor.magic_spirit"
                            })
                        } else {
                            self.guard_names(pos).first().cloned().unwrap_or_default()
                        });
                    }
                }
                FeatureKind::Encounter | FeatureKind::Tower => {
                    let guards = self.guard_names(pos);
                    text.special.extend(guards);
                }
                FeatureKind::Mineral => {}
            }
        }

        text.blocked = match survey.blocked {
            Some(FoundCityError::BadTerrain) if def.class == TerrainClass::Ocean => {
                Some(self.text("surveyor.blocked_water"))
            }
            Some(FoundCityError::BadTerrain) => Some(self.text("surveyor.blocked_land")),
            Some(FoundCityError::BlockedByFeature) => {
                Some(self.text(match feature.map(|f| f.kind) {
                    Some(FeatureKind::Tower) => "surveyor.blocked_tower",
                    Some(FeatureKind::Node) => "surveyor.blocked_node",
                    _ => "surveyor.blocked_lair",
                }))
            }
            Some(FoundCityError::TooCloseToCity) => Some(self.text_with(
                "surveyor.blocked_close",
                &Args::new().count("n", content.rules.config.min_city_distance),
            )),
            _ => None,
        };
        if text.blocked.is_none() || city.is_some() {
            let (size, prod, gold) = match city.and_then(|c| self.game.economy().city_economy(c)) {
                Some(e) => (e.max_size, survey.production_percent, survey.gold_percent),
                None => (
                    survey.max_size,
                    survey.production_percent,
                    survey.gold_percent,
                ),
            };
            text.resources = Some((size, prod, gold));
            text.blocked = None;
        }
        Some(text)
    }

    /// Names of the units guarding a site, when the square is in sight now.
    fn guard_names(&self, pos: Position) -> Vec<String> {
        let width = self.game.map().map_or(0, |m| m.width());
        let seen = self
            .game
            .economy()
            .visibility(HUMAN, pos.plane)
            .get((pos.y * width + pos.x) as usize)
            .copied()
            .unwrap_or(false);
        if self.game.economy().lair_at(pos).is_none() {
            return Vec::new();
        }
        if !seen {
            return vec![self.text("ui.unexplored")];
        }
        // The guards are known to whoever has fought for the site once.
        let Some(guards) = self.game.economy().lair_guards_known_to(pos, HUMAN) else {
            return vec![self.text("surveyor.guards_unknown")];
        };
        let rules = &self.game.content().rules;
        guards
            .iter()
            .take(2)
            .filter_map(|(id, _)| {
                rules
                    .units
                    .id_of(id)
                    .map(|u| rules.units.get(u).name.clone())
            })
            .collect()
    }

    /// The panel over the side column, and its Cancel button.
    pub(crate) fn surveyor_bar(&mut self, input: &Input) {
        if !self.surveyor {
            return;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let art = self.draw_pic(&lay, "main", 44, 0, 242.0, 173.0).is_some()
            && self.draw_pic(&lay, "main", 57, 0, 240.0, 76.0).is_some();
        self.draw_pic(&lay, "main", 47, 0, 240.0, 173.0);
        let cancel = self.pic_button_on(&lay, ("main", 41), CANCEL_AT, input);
        let mut leave =
            input.key_pressed(KeyCode::C) || input.key_pressed(KeyCode::Escape) || cancel;
        if !art {
            self.log.push(self.text("surveyor.needs_pictures"));
            leave = true;
        }
        let title = self.text("surveyor.title");
        self.draw_styled(view, &Style::set(4, 1).center(), &title, 280.0, 80.0);
        let text = self
            .tile_under_mouse(input)
            .and_then(|pos| self.describe_square(pos));
        if let Some(t) = text {
            self.draw_styled(view, &Style::set(1, 1).center(), &t.terrain, 280.0, 91.0);
            for (i, line) in t.effects.iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::set(0, 0).center(),
                    line,
                    280.0,
                    98.0 + 7.0 * i as f32,
                );
            }
            for (i, line) in t.special.iter().enumerate() {
                let set = usize::from(i == 0);
                self.draw_styled(
                    view,
                    &Style::set(1, set).center(),
                    line,
                    280.0,
                    114.0 + 7.0 * i as f32,
                );
            }
            if !t.detail.is_empty() {
                let y = 121.0 + 7.0 * t.special.len().saturating_sub(1) as f32;
                for (i, line) in self.bm_wrap(0, &t.detail, 66.0).iter().enumerate() {
                    self.draw_styled(view, &Style::set(0, 0), line, 247.0, y + 7.0 * i as f32);
                }
            }
            if let Some((size, prod, gold)) = t.resources {
                let (resources, max_pop) = (
                    self.text("surveyor.city_resources"),
                    self.text("surveyor.max_pop"),
                );
                let (prod_bonus, gold_bonus) = (
                    self.text("surveyor.prod_bonus"),
                    self.text("surveyor.gold_bonus"),
                );
                self.draw_styled(view, &Style::set(1, 1).center(), &resources, 280.0, 143.0);
                self.draw_styled(view, &Style::set(0, 0), &max_pop, 245.0, 151.0);
                self.draw_styled(
                    view,
                    &Style::set(0, 0).right(),
                    &size.to_string(),
                    306.0,
                    151.0,
                );
                self.draw_styled(view, &Style::set(0, 0), &prod_bonus, 245.0, 158.0);
                self.draw_styled(
                    view,
                    &Style::set(0, 0).right(),
                    &format!("+{prod}%"),
                    312.0,
                    158.0,
                );
                self.draw_styled(view, &Style::set(0, 0), &gold_bonus, 245.0, 165.0);
                self.draw_styled(
                    view,
                    &Style::set(0, 0).right(),
                    &format!("+{gold}%"),
                    312.0,
                    165.0,
                );
            } else if let Some(why) = t.blocked {
                let message = why;
                for (i, line) in self.bm_wrap(0, &message, 67.0).iter().enumerate() {
                    self.draw_styled(view, &Style::set(0, 0), line, 246.0, 143.0 + 7.0 * i as f32);
                }
            }
        }
        if leave {
            self.stop_surveyor();
        }
    }

    /// The map square under the mouse, if it is over the map.
    pub(crate) fn tile_under_mouse(&self, input: &Input) -> Option<Position> {
        self.survey_probe
            .or_else(|| self.tile_at(input.mouse_position().into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_reads_in_halves() {
        let t = crate::test_app::shipped::texts();
        assert_eq!(food_text(t, 0), "");
        assert_eq!(food_text(t, 1), "1/2 food");
        assert_eq!(food_text(t, 2), "1 food");
        assert_eq!(food_text(t, 3), "1 1/2 food");
        assert_eq!(food_text(t, 4), "2 food");
    }
}
