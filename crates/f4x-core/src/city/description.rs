//! What a building says it does. The numbers come from the building's own effects in
//! `buildings.ron`, the data the rules read, so the text cannot drift from the rules; the
//! building's flavour sentence (`building.<id>`) adds what no effect in the data carries.

use crate::content::Content;
use crate::rules::effects::{Effect, EffectRule, Trigger};
use crate::rules::{BuildingId, ModOp, Stat};
use crate::text::Args;

/// The flavour sentence of each building (the keys are in `content/text/en/building.ron`).
const FLAVOUR: [(&str, &str); 33] = [
    ("barracks", "building.barracks"),
    ("armory", "building.armory"),
    ("fighters_guild", "building.fighters_guild"),
    ("armorers_guild", "building.armorers_guild"),
    ("war_college", "building.war_college"),
    ("smithy", "building.smithy"),
    ("stables", "building.stables"),
    ("animists_guild", "building.animists_guild"),
    ("fantastic_stable", "building.fantastic_stable"),
    ("ship_wrights_guild", "building.ship_wrights_guild"),
    ("ship_yard", "building.ship_yard"),
    ("maritime_guild", "building.maritime_guild"),
    ("sawmill", "building.sawmill"),
    ("library", "building.library"),
    ("sages_guild", "building.sages_guild"),
    ("oracle", "building.oracle"),
    ("alchemists_guild", "building.alchemists_guild"),
    ("university", "building.university"),
    ("wizards_guild", "building.wizards_guild"),
    ("shrine", "building.shrine"),
    ("temple", "building.temple"),
    ("parthenon", "building.parthenon"),
    ("cathedral", "building.cathedral"),
    ("marketplace", "building.marketplace"),
    ("bank", "building.bank"),
    ("merchants_guild", "building.merchants_guild"),
    ("granary", "building.granary"),
    ("farmers_market", "building.farmers_market"),
    ("foresters_guild", "building.foresters_guild"),
    ("builders_hall", "building.builders_hall"),
    ("mechanicians_guild", "building.mechanicians_guild"),
    ("miners_guild", "building.miners_guild"),
    ("city_walls", "building.city_walls"),
];

/// The key of the sentence for an effect and the number it shows, or `None` when the text has
/// no sentence for that kind of effect (a test keeps every shipped effect covered).
pub fn effect_key(rule: &EffectRule) -> Option<(&'static str, i64)> {
    if rule.when != Trigger::Passive {
        return None;
    }
    let Effect::Modify { stat, op, value } = rule.effect else {
        return None;
    };
    let n = i64::from(value);
    let key = match (stat, op) {
        (Stat::CityResearch, ModOp::Add) => "building.effect_research",
        (Stat::CityMana, ModOp::Add) => "building.effect_mana",
        (Stat::CityUnrest, ModOp::Add) if value < 0 => {
            return Some(("building.effect_unrest", -n));
        }
        (Stat::CityProduction, ModOp::Percent) => "building.effect_production",
        (Stat::CityGold, ModOp::Percent) => "building.effect_gold",
        (Stat::CityFood, ModOp::Add) | (Stat::CityFoodFlat, ModOp::Add) => "building.effect_food",
        (Stat::FarmerFood, ModOp::Add) => "building.effect_farmer_food",
        (Stat::CityGrowth, ModOp::Add) => "building.effect_growth",
        (Stat::StartingXp, ModOp::Min) => "building.effect_starting_xp",
        (Stat::CitySight, ModOp::Min) => "building.effect_sight",
        _ => return None,
    };
    Some((key, n))
}

/// The description of a building: its effects with their numbers, its flavour sentence, and
/// the buildings it needs.
pub fn building_description(content: &Content, building: BuildingId) -> String {
    let def = content.rules.buildings.get(building);
    let texts = &content.texts;
    let mut parts: Vec<String> = def
        .effects
        .iter()
        .filter_map(effect_key)
        .map(|(key, n)| texts.get(key, &Args::new().count("n", n)))
        .collect();
    let mut out = String::new();
    if !parts.is_empty() {
        out.push_str(&parts.join(", "));
        out.push('.');
    }
    if let Some((_, key)) = FLAVOUR.iter().find(|(id, _)| *id == def.id) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&texts.get(key, &Args::new()));
    }
    parts = def
        .requires
        .iter()
        .filter_map(|id| content.rules.buildings.by_name(id))
        .map(|b| b.name.clone())
        .collect();
    if let Some(last) = parts.pop() {
        let list = if parts.is_empty() {
            last
        } else {
            format!(
                "{} {} {last}",
                parts.join(", "),
                texts.get("production.and", &Args::new())
            )
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&texts.get("building.needs", &Args::new().text("list", list)));
    }
    out
}
