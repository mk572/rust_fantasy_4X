//! Where `extract-art` puts a picture: the ones that belong to one screen alone (`screens.tsv`) in
//! `assets-local/screens/<screen>/`, unit art (overland picture, combat figures, hero portrait) in
//! `assets-local/art/units/<unit id>/`, a spell's pictures (`content/spell_art.ron`) in
//! `assets-local/art/spells/<spell id>/` (`_shared` for those of several spells, `_book` for the
//! spell book), and the rest in `assets-local/art/<lbx>/`.

use std::collections::{BTreeMap, HashMap};

use f4x_data::spell_art::{self, SpellArt};

/// The table, compiled in so the extractor needs no extra file.
const TABLE: &str = include_str!("../screens.tsv");

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub lbx: String,
    pub first: usize,
    pub last: usize,
    pub screen: String,
    pub label: Option<String>,
}

/// Where a screen picture goes: `<screen>/<name>`, the file being `<name>_<frame>.png`.
pub fn placement(rules: &[Rule], lbx: &str, entry: usize) -> Option<String> {
    let rule = rules
        .iter()
        .find(|r| r.lbx == lbx && (r.first..=r.last).contains(&entry))?;
    let name = match (&rule.label, rule.first == rule.last) {
        (Some(label), true) => label.clone(),
        _ => format!("{lbx}_{entry:03}"),
    };
    Some(format!("{}/{name}", rule.screen))
}

/// The picture index at which `SPECIAL2.LBX` continues the ability icons of `SPECIAL.LBX`.
const SPECIAL2_FIRST_ICON: usize = 111;
/// How many entries at the start of `SPECIAL2.LBX` are ability icons.
const ABILITY_ICONS_IN_SPECIAL2: usize = 34;

/// The placement of every picture that has a special place.
pub struct Placer {
    rules: Vec<Rule>,
    /// Original unit type number -> unit id.
    types: HashMap<usize, String>,
    /// `PORTRAIT.LBX` entry -> hero unit id.
    portraits: HashMap<usize, String>,
    /// `MAPBACK.LBX` entry -> the map sprite's name (`map_art::catalog_names`).
    map_sprites: HashMap<usize, String>,
    /// `(lbx, entry)` -> the folder under `art/spells/` (`SpellArt::folders`).
    spells: BTreeMap<(String, usize), String>,
}

/// What the placer needs to know of a unit: its id, its art `(file, entry)` and its portrait.
pub struct UnitArt {
    pub id: String,
    pub art: Option<(String, usize)>,
    pub portrait: Option<usize>,
}

impl Placer {
    pub fn new(rules: Vec<Rule>, units: &[UnitArt], spell_art: &SpellArt) -> Self {
        let mut types = HashMap::new();
        let mut portraits = HashMap::new();
        for u in units {
            if let Some((lbx, entry)) = &u.art {
                match lbx.as_str() {
                    "units1" => types.insert(*entry, u.id.clone()),
                    "units2" => types.insert(120 + entry, u.id.clone()),
                    _ => None,
                };
            }
            if let Some(p) = u.portrait {
                portraits.insert(p, u.id.clone());
            }
        }
        let map_sprites = crate::map_art::catalog_names()
            .into_iter()
            .map(|(name, entry)| (entry, name))
            .collect();
        Self {
            rules,
            types,
            portraits,
            map_sprites,
            spells: spell_art.folders(),
        }
    }

    /// Where a picture goes, relative to `assets-local/` and without the frame suffix
    /// (`<place>_<frame>.png`), or `None` to leave it in `art/<lbx>/`.
    pub fn place(&self, lbx: &str, entry: usize) -> Option<String> {
        if lbx == "spells" {
            return Some(format!("art/spells/{}/{lbx}_{entry:03}", spell_art::BOOK));
        }
        // `MONSTER.LBX` holds the pictures the summon scene shows, one for each summoned creature
        // (entry = unit number - the Magic Spirit's), the heroes and the sword.
        if lbx == "monster" {
            return Some(format!("art/summoning/{entry:03}"));
        }
        // The 16x16 ability icons of the unit sheet: `SPECIAL.LBX` and the first 34 entries of
        // `SPECIAL2.LBX`, filed by the picture index of the original's `special_seg` (the
        // second file continues at 111). The rest of `SPECIAL2.LBX` is not an icon.
        match lbx {
            "special" => return Some(format!("art/unit_ability/{entry:03}")),
            "special2" if entry < ABILITY_ICONS_IN_SPECIAL2 => {
                return Some(format!(
                    "art/unit_ability/{:03}",
                    entry + SPECIAL2_FIRST_ICON
                ));
            }
            _ => {}
        }
        if let Some(folder) = self.spells.get(&(lbx.to_string(), entry)) {
            return Some(format!("art/spells/{folder}/{lbx}_{entry:03}"));
        }
        if let Some(place) = placement(&self.rules, lbx, entry) {
            return Some(format!("screens/{place}"));
        }
        // The battlefield's ground, trees and rocks: four terrains on Arcanus (`cmbgrass`...) and
        // Myrror (`cmbgrasc`...), and two sets (`desert`, `tundra`) the original never loads.
        let terrain = match lbx {
            "cmbgrass" | "cmbgrasc" => Some("grassland"),
            "cmbdesrt" | "cmbdesrc" | "desert" => Some("desert"),
            "cmbmount" | "cmbmounc" => Some("mountains"),
            "cmbtundr" | "cmbtundc" | "tundra" => Some("tundra"),
            _ => None,
        };
        if let Some(terrain) = terrain {
            return Some(format!("art/battle/tiles/{terrain}/{lbx}_{entry:03}"));
        }
        let unit = |ty: usize, name: String| {
            self.types
                .get(&ty)
                .map(|id| format!("art/units/{id}/{name}"))
        };
        match lbx {
            "units1" => unit(entry, "picture".into()),
            "units2" => unit(120 + entry, "picture".into()),
            "mapback" => self
                .map_sprites
                .get(&entry)
                .map(|name| format!("art/map/{}/{name}", crate::map_art::folder(name))),
            "portrait" => self
                .portraits
                .get(&entry)
                .map(|id| format!("art/units/{id}/portrait")),
            _ => {
                // `figures1` to `figures9`, `figure10` to `figure16`: fifteen unit types a file,
                // eight directions each.
                let n: usize = lbx
                    .strip_prefix("figures")
                    .or_else(|| lbx.strip_prefix("figure"))?
                    .parse()
                    .ok()?;
                unit((n - 1) * 15 + entry / 8, format!("figure_{}", entry % 8))
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let bad = |what: &str| format!("screens.tsv line {}: {what}", n + 1);
        if f.len() < 4 {
            return Err(bad("needs lbx, first, last and screen"));
        }
        let first: usize = f[1].parse().map_err(|_| bad("first is not a number"))?;
        let last: usize = f[2].parse().map_err(|_| bad("last is not a number"))?;
        if first > last {
            return Err(bad("first is after last"));
        }
        rules.push(Rule {
            lbx: f[0].to_string(),
            first,
            last,
            screen: f[3].to_string(),
            label: f.get(4).filter(|l| !l.is_empty()).map(|l| l.to_string()),
        });
    }
    Ok(rules)
}

/// The compiled-in table.
pub fn rules() -> Result<Vec<Rule>, String> {
    parse(TABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_art_goes_to_the_units_folder() {
        let units = [
            UnitArt {
                id: "dwarf".into(),
                art: Some(("units1".into(), 0)),
                portrait: Some(10),
            },
            UnitArt {
                id: "djinn".into(),
                art: Some(("units2".into(), 75)),
                portrait: None,
            },
        ];
        let p = Placer::new(rules().unwrap(), &units, &SpellArt::default());
        assert_eq!(
            p.place("units1", 0).as_deref(),
            Some("art/units/dwarf/picture")
        );
        assert_eq!(
            p.place("portrait", 10).as_deref(),
            Some("art/units/dwarf/portrait")
        );
        assert_eq!(
            p.place("units2", 75).as_deref(),
            Some("art/units/djinn/picture")
        );
        // Type 0 is entries 0 to 7 of figures1; type 15 starts figures2.
        assert_eq!(
            p.place("figures1", 3).as_deref(),
            Some("art/units/dwarf/figure_3")
        );
        assert_eq!(p.place("figures1", 8), None);
        assert_eq!(
            p.place("main", 8).as_deref(),
            Some("screens/main_screen/button_done")
        );
        assert_eq!(p.place("terrain", 0), None);
        assert_eq!(
            p.place("mapback", 71).as_deref(),
            Some("art/map/locations/site_cave")
        );
        assert_eq!(
            p.place("mapback", 78).as_deref(),
            Some("art/map/overlays/resources/mineral_coal")
        );
        assert_eq!(p.place("mapback", 150), None);
        assert_eq!(
            p.place("citywall", 3).as_deref(),
            Some("screens/combat/walls/citywall_003")
        );
        assert_eq!(
            p.place("cityscap", 5).as_deref(),
            Some("screens/city_screen/cityscape/cityscap_005")
        );
        assert_eq!(
            p.place("compix", 4).as_deref(),
            Some("screens/combat/interface/button_auto")
        );
        assert_eq!(
            p.place("cmbdesrc", 7).as_deref(),
            Some("art/battle/tiles/desert/cmbdesrc_007")
        );
        assert_eq!(
            p.place("tundra", 50).as_deref(),
            Some("art/battle/tiles/tundra/tundra_050")
        );
        assert_eq!(
            p.place("cmbtwall", 1).as_deref(),
            Some("screens/combat/cmbtwall_unknown/cmbtwall_001")
        );
    }

    #[test]
    fn spell_art_goes_to_the_spell_folder_or_the_shared_one() {
        let art = f4x_data::spell_art::parse_spell_art(
            r#"(
                shared: [(lbx: "specfx", entry: 49, role: Sparkle)],
                spells: {
                    "fire_bolt": [(lbx: "cmbtfx", entry: 0, role: Cast)],
                    "petrify": [(lbx: "specfx", entry: 12, role: Cast)],
                    "call_the_void": [(lbx: "specfx", entry: 12, role: Cast)],
                },
            )"#,
        )
        .unwrap();
        let p = Placer::new(rules().unwrap(), &[], &art);
        assert_eq!(
            p.place("cmbtfx", 0).as_deref(),
            Some("art/spells/fire_bolt/cmbtfx_000")
        );
        assert_eq!(
            p.place("specfx", 12).as_deref(),
            Some("art/spells/_shared/specfx_012")
        );
        assert_eq!(
            p.place("specfx", 49).as_deref(),
            Some("art/spells/_shared/specfx_049")
        );
        assert_eq!(
            p.place("spells", 3).as_deref(),
            Some("art/spells/_book/spells_003")
        );
        // A picture no spell names stays in its file's folder.
        assert_eq!(p.place("specfx", 100), None);
    }

    #[test]
    fn the_summoned_creatures_pictures_go_to_the_summoning_folder() {
        let p = Placer::new(
            rules().unwrap(),
            &[],
            &f4x_data::spell_art::parse_spell_art("(shared: [], spells: {})").unwrap(),
        );
        assert_eq!(p.place("monster", 10).as_deref(), Some("art/summoning/010"));
    }

    #[test]
    fn the_ability_icons_of_both_special_files_share_one_folder() {
        let p = Placer::new(
            rules().unwrap(),
            &[],
            &f4x_data::spell_art::parse_spell_art("(shared: [], spells: {})").unwrap(),
        );
        assert_eq!(
            p.place("special", 5).as_deref(),
            Some("art/unit_ability/005")
        );
        assert_eq!(
            p.place("special2", 0).as_deref(),
            Some("art/unit_ability/111")
        );
        assert_eq!(
            p.place("special2", 33).as_deref(),
            Some("art/unit_ability/144")
        );
        assert_eq!(p.place("special2", 34), None);
    }

    #[test]
    fn no_spell_picture_is_also_claimed_by_a_screen() {
        let dir = std::path::Path::new("../../content");
        let content = f4x_data::load_content(dir).unwrap();
        let art = f4x_data::spell_art::load_spell_art(&[dir], &content.rules).unwrap();
        let rules = rules().unwrap();
        for (lbx, entry) in art.folders().keys() {
            assert_eq!(
                placement(&rules, lbx, *entry),
                None,
                "{lbx} {entry} is in screens.tsv and in spell_art.ron"
            );
        }
    }

    #[test]
    fn the_table_parses_and_no_entry_is_claimed_twice() {
        let rules = rules().unwrap();
        assert!(rules.len() > 50);
        for (i, a) in rules.iter().enumerate() {
            assert!(
                a.screen
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '/'),
                "{a:?}"
            );
            for b in &rules[i + 1..] {
                let overlap = a.lbx == b.lbx && a.first <= b.last && b.first <= a.last;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn a_labelled_single_entry_is_named_and_a_range_numbered() {
        let rules = rules().unwrap();
        assert_eq!(
            placement(&rules, "main", 8).as_deref(),
            Some("main_screen/button_done")
        );
        assert_eq!(
            placement(&rules, "main", 16).as_deref(),
            Some("main_screen/main_016")
        );
        assert_eq!(
            placement(&rules, "main", 45).as_deref(),
            Some("road_build/background")
        );
        // Shared pictures stay where they were.
        assert_eq!(placement(&rules, "backgrnd", 24), None);
        assert_eq!(placement(&rules, "load", 8), None);
        // A picture two screens use lives with its main screen.
        assert_eq!(
            placement(&rules, "armylist", 1).as_deref(),
            Some("city_list/button_up")
        );
        assert_eq!(
            placement(&rules, "load", 4).as_deref(),
            Some("load_save/button_ok")
        );
        assert_eq!(placement(&rules, "terrain", 0), None);
    }

    #[test]
    fn bad_rows_are_refused() {
        assert!(parse("main\t1\n").is_err());
        assert!(parse("main\t9\t1\tx\n").is_err());
        assert!(parse("main\ta\t1\tx\n").is_err());
    }
}
