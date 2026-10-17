//! The Surveyor (task 71): `Surveyor_Screen` and `Surveyor_Window_Display` in `Surveyor.c`.
//! F1 puts the side panel over the right-hand column; the square under the mouse is described:
//! its terrain, what lies on it, and what a city founded there would get from the land around
//! it, or why none can be founded. A click on the map centres it there, and Cancel, C or Escape
//! (or a right-click) leave.

use f4x_core::city::{FoundCityError, Position};
use f4x_core::content::{FeatureKind, TerrainClass};
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, HUMAN};

const CANCEL_AT: (f32, f32) = (263.0, 181.0);
const BUTTON_SIZE: (f32, f32) = (33.0, 9.0);

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
pub(crate) fn food_text(half_units: u8) -> String {
    match (half_units / 2, half_units % 2) {
        (0, 0) => String::new(),
        (0, _) => "1/2 food".into(),
        (w, 0) => format!("{w} food"),
        (w, _) => format!("{w} 1/2 food"),
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
        text.effects[0] = food_text(food.clamp(0, 255) as u8);
        let production =
            self.game
                .economy()
                .square_production(grid, pos.x as i32, pos.y as i32, false);
        if production != 0 {
            text.effects[1] = format!("+{production}% production");
        }
        if grid.has_river(pos.x as i32, pos.y as i32) && survey.gold_percent >= 20 {
            let gold = format!("+{}% gold", survey.gold_percent);
            if text.effects[1].is_empty() {
                text.effects[1] = gold;
            } else {
                text.effects[0] = format!("{}, {gold}", text.effects[0]);
            }
        } else if survey.gold_percent > 0 && def.class != TerrainClass::Ocean {
            text.effects[1] = format!("+{}% gold", survey.gold_percent);
        }
        if spoiled {
            text.effects = ["Corruption".into(), String::new()];
        }

        let feature = grid
            .feature(pos.x as i32, pos.y as i32)
            .and_then(|f| content.features.get(f));
        let city = self.game.economy().city_at(pos);
        if let Some(city) = city {
            let figures = self
                .game
                .economy()
                .city_economy(city)
                .map_or(0, |e| e.figures);
            let size = if figures == 0 {
                "Outpost"
            } else {
                ["Hamlet", "Village", "Town", "City", "Capital"]
                    [(figures.div_ceil(4).clamp(1, 5) - 1) as usize]
            };
            text.special = vec![format!("{size} of"), self.city_name(city)];
        } else if let Some(f) = feature {
            text.special.push(f.name.clone());
            let b = f.city;
            let mut bits = Vec::new();
            if b.gold > 0 {
                bits.push(format!("+{} gold", b.gold));
            }
            if b.mana > 0 {
                bits.push(format!("+{} power", b.mana));
            }
            if b.food > 0 {
                bits.push(format!("+{} food", b.food));
            }
            if b.unit_cost_percent > 0 {
                bits.push(format!(
                    "Reduces normal unit cost by {}%",
                    b.unit_cost_percent
                ));
            }
            text.detail = bits.join(", ");
            match f.kind {
                FeatureKind::Node => {
                    if let Some(i) = map.node_at(pos.plane, pos.x, pos.y) {
                        let n = &map.nodes()[i];
                        let realm = n.realm.clone();
                        let realm = realm[..1].to_uppercase() + &realm[1..];
                        text.special = vec![format!("{realm} Node")];
                        text.special.push(if n.owner.is_some() {
                            if n.warped {
                                "Warped"
                            } else if n.guardian {
                                "Guardian Spirit"
                            } else {
                                "Magic Spirit"
                            }
                            .to_string()
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
                Some("on water.".into())
            }
            Some(FoundCityError::BadTerrain) => Some("on this kind of land.".into()),
            Some(FoundCityError::BlockedByFeature) => Some(
                match feature.map(|f| f.kind) {
                    Some(FeatureKind::Tower) => "on towers.",
                    Some(FeatureKind::Node) => "on magic nodes.",
                    _ => "on lairs.",
                }
                .into(),
            ),
            Some(FoundCityError::TooCloseToCity) => Some(format!(
                "less than {} squares from any other city.",
                content.rules.config.min_city_distance
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
        let Some((_, lair)) = self.game.economy().lair_at(pos) else {
            return Vec::new();
        };
        if !seen {
            return vec!["Unexplored".into()];
        }
        let rules = &self.game.content().rules;
        lair.guards
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
    pub(crate) fn surveyor_bar(&mut self) {
        if !self.surveyor {
            return;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let mouse: Vec2 = mouse_position().into();
        let cancel = lay.rect((CANCEL_AT.0, CANCEL_AT.1, BUTTON_SIZE.0, BUTTON_SIZE.1));
        let art = self.draw_pic(&lay, "main", 44, 0, 242.0, 173.0).is_some()
            && self.draw_pic(&lay, "main", 57, 0, 240.0, 76.0).is_some();
        self.draw_pic(&lay, "main", 47, 0, 240.0, 173.0);
        let down = is_mouse_button_down(MouseButton::Left);
        let frame = usize::from(down && cancel.contains(mouse));
        self.draw_pic(&lay, "main", 41, frame, CANCEL_AT.0, CANCEL_AT.1);
        let mut leave = is_key_pressed(KeyCode::C)
            || is_key_pressed(KeyCode::Escape)
            || (is_mouse_button_pressed(MouseButton::Left) && cancel.contains(mouse));
        if !art {
            self.log
                .push("The Surveyor needs the extracted pictures".into());
            leave = true;
        }
        self.bm_text_set(view, 4, 1, "Surveyor", 280.0, 80.0, Align::Center);
        let text = self
            .tile_under_mouse()
            .and_then(|pos| self.describe_square(pos));
        if let Some(t) = text {
            self.bm_text_set(view, 1, 1, &t.terrain, 280.0, 91.0, Align::Center);
            for (i, line) in t.effects.iter().enumerate() {
                self.bm_text_set(
                    view,
                    0,
                    0,
                    line,
                    280.0,
                    98.0 + 7.0 * i as f32,
                    Align::Center,
                );
            }
            for (i, line) in t.special.iter().enumerate() {
                let set = usize::from(i == 0);
                self.bm_text_set(
                    view,
                    1,
                    set,
                    line,
                    280.0,
                    114.0 + 7.0 * i as f32,
                    Align::Center,
                );
            }
            if !t.detail.is_empty() {
                let y = 121.0 + 7.0 * t.special.len().saturating_sub(1) as f32;
                for (i, line) in self.bm_wrap(0, &t.detail, 66.0).iter().enumerate() {
                    self.bm_text_set(view, 0, 0, line, 247.0, y + 7.0 * i as f32, Align::Left);
                }
            }
            if let Some((size, prod, gold)) = t.resources {
                self.bm_text_set(view, 1, 1, "City Resources", 280.0, 143.0, Align::Center);
                self.bm_text_set(view, 0, 0, "Maximum Pop", 245.0, 151.0, Align::Left);
                self.bm_text_set(view, 0, 0, &size.to_string(), 306.0, 151.0, Align::Right);
                self.bm_text_set(view, 0, 0, "Prod Bonus", 245.0, 158.0, Align::Left);
                self.bm_text_set(view, 0, 0, &format!("+{prod}%"), 312.0, 158.0, Align::Right);
                self.bm_text_set(view, 0, 0, "Gold Bonus", 245.0, 165.0, Align::Left);
                self.bm_text_set(view, 0, 0, &format!("+{gold}%"), 312.0, 165.0, Align::Right);
            } else if let Some(why) = t.blocked {
                let message = format!("Cities cannot be built {why}");
                for (i, line) in self.bm_wrap(0, &message, 67.0).iter().enumerate() {
                    self.bm_text_set(view, 0, 0, line, 246.0, 143.0 + 7.0 * i as f32, Align::Left);
                }
            }
        }
        if leave {
            self.stop_surveyor();
        }
    }

    /// The map square under the mouse, if it is over the map.
    pub(crate) fn tile_under_mouse(&self) -> Option<Position> {
        self.survey_probe
            .or_else(|| self.tile_at(mouse_position().into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_reads_in_halves() {
        assert_eq!(food_text(0), "");
        assert_eq!(food_text(1), "1/2 food");
        assert_eq!(food_text(2), "1 food");
        assert_eq!(food_text(3), "1 1/2 food");
        assert_eq!(food_text(4), "2 food");
    }
}
