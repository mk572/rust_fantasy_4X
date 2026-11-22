//! How the AI weighs a place for a city (`crates/f4x-def/content/ai_sites.ron`): what each kind of
//! ground, special square and neighbour is worth to a site, in points. The numbers are AI tuning,
//! not rules of the game, so a rule set may change them (or leave the file out for these).
//!
//! The defaults are the numbers the AI shipped with, and `ai_sites.ron` repeats them; a test keeps
//! the two equal.
//!
//! Tests: in `tests/it/c1_content.rs` (the shipped file is the default; bad values are refused) and
//! `f4x-ai/src/city_site.rs` (each weight moves the score the way its name says).

use serde::{Deserialize, Serialize};

use crate::map::TerrainClass;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteRules {
    /// What a square of each terrain is worth to the city that works it (food and growth). A
    /// class not listed is worth nothing.
    pub land_values: Vec<(TerrainClass, f32)>,
    /// A square that another city works too gives only this share of its value (0 to 1).
    pub shared_share: f32,
    /// Per point of the production the land gives (percent over the area).
    pub production: f32,
    /// Per point of mana a special square gives.
    pub mana: f32,
    /// Per point of gold a special square gives.
    pub gold: f32,
    /// Per point of food a special square gives.
    pub food: f32,
    /// Per percent off the cost of soldiers (coal, iron).
    pub unit_cost: f32,
    /// Per river square.
    pub river: f32,
    /// The most a site gains from standing next to the wizard's own cities, and what each square
    /// of distance takes off.
    pub near_own: f32,
    pub near_own_step: f32,
    /// A rival's city within this many squares lowers the site by `enemy_peace` for each square
    /// it is nearer than the range, or `enemy_war` when the rival is at war with the wizard.
    pub enemy_range: u32,
    pub enemy_peace: f32,
    pub enemy_war: f32,
    /// What a site over the sea costs against one on the home landmass.
    pub sea_cost: f32,
    /// Sites that need a crossing count this many at most when the wizard asks how many settlers
    /// it has work for: a few overseas, so that the home land is filled first.
    pub crossing_sites_counted: usize,
}

impl SiteRules {
    /// What a square of `class` is worth (food and growth); nothing for no square.
    pub fn land_value(&self, class: Option<TerrainClass>) -> f32 {
        class
            .and_then(|c| self.land_values.iter().find(|(k, _)| *k == c))
            .map_or(0.0, |&(_, v)| v)
    }

    /// The problems of the numbers: weights that are not finite or below zero, a share outside
    /// 0 to 1, no range.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let named = [
            ("shared_share", self.shared_share),
            ("production", self.production),
            ("mana", self.mana),
            ("gold", self.gold),
            ("food", self.food),
            ("unit_cost", self.unit_cost),
            ("river", self.river),
            ("near_own", self.near_own),
            ("near_own_step", self.near_own_step),
            ("enemy_peace", self.enemy_peace),
            ("enemy_war", self.enemy_war),
            ("sea_cost", self.sea_cost),
        ];
        for (name, v) in named {
            if !v.is_finite() || v < 0.0 {
                p.push(format!(
                    "ai_sites.ron: {name} must be a number of 0 or more"
                ));
            }
        }
        if self.shared_share > 1.0 {
            p.push("ai_sites.ron: shared_share is a share, 0 to 1".into());
        }
        if self.enemy_range == 0 {
            p.push("ai_sites.ron: enemy_range must be at least 1".into());
        }
        if self
            .land_values
            .iter()
            .any(|&(_, v)| !v.is_finite() || v < 0.0)
        {
            p.push("ai_sites.ron: a land value must be a number of 0 or more".into());
        }
        p
    }
}

impl Default for SiteRules {
    fn default() -> Self {
        use TerrainClass::*;
        Self {
            land_values: vec![
                (Grassland, 3.0),
                (SorceryNode, 3.0),
                (Forest, 2.0),
                (NatureNode, 2.0),
                (Hills, 2.0),
                (Swamp, 1.0),
                (Mountains, 1.0),
                (ChaosNode, 1.0),
                (Desert, 0.5),
                (Tundra, 0.5),
                (Ocean, 0.7),
            ],
            shared_share: 0.4,
            production: 0.1,
            mana: 1.5,
            gold: 1.0,
            food: 1.0,
            unit_cost: 0.2,
            river: 0.5,
            near_own: 6.0,
            near_own_step: 0.8,
            enemy_range: 8,
            enemy_peace: 0.5,
            enemy_war: 1.5,
            sea_cost: 10.0,
            crossing_sites_counted: 2,
        }
    }
}
