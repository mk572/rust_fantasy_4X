//! The boxes of a guarded site (task 128, ReMoM `Lair.c`): "You have found a mysterious cave.
//! Scouts have spotted 4 Skeletons within the mysterious cave.  Do you wish to enter?" with a
//! Yes and a No (`Do_Lair_Confirm`), and after the fight "Inside you find 120 gold and a Sword."
//! (`Lair_Treasure_Popup`, the original's `Notify1` box with the treasure picture). An unguarded
//! site shows the first box without buttons on arrival. The boxes come one after another over
//! the map, in the order the events arrived.

use f4x_core::feature::lair::Loot;
use f4x_core::game::Command;
use f4x_core::map::Position;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen};

/// What a site is called, its picture in `reload` (`9 +` the type) and how it is introduced, in
/// the order of the original's table `TBL_EZ_Names`.
const SITES: [(&str, &str, &str); 11] = [
    ("tower", "tower", "a"),
    ("chaos_node", "chaos node", "a"),
    ("nature_node", "nature node", "a"),
    ("sorcery_node", "sorcery node", "a"),
    ("mysterious_cave", "mysterious cave", "a"),
    ("dungeon", "dungeon", "a"),
    ("ancient_temple", "ancient temple", "an"),
    ("abandoned_keep", "abandoned keep", "an"),
    ("monster_lair", "monster lair", "a"),
    ("ruins", "ruins", "some"),
    ("fallen_temple", "fallen temple", "a"),
];

/// The site's place in the original's table; a site it does not know is a lair.
pub(crate) fn site_type(site: &str) -> usize {
    SITES.iter().position(|s| s.0 == site).unwrap_or(8)
}

/// One box waiting to be shown.
#[derive(Debug, Clone)]
pub enum SiteNotice {
    /// A guarded site is in the way: fight for it?
    Ask {
        units: Vec<hecs::Entity>,
        to: Position,
        text: String,
        site: usize,
    },
    /// An unguarded site has been found: no choice to make.
    Found { text: String, site: usize },
    /// What the cleared site held.
    Treasure { text: String },
}

/// The first line of the question: "You have found a mysterious cave." and, for a guarded site,
/// what the scouts saw.
pub(crate) fn found_text(site: usize, guards: Option<(&str, u32)>) -> String {
    let (_, name, article) = SITES[site];
    let mut text = format!("You have found {article} {name}.");
    if let Some((unit, figures)) = guards {
        text.push_str("  Scouts have spotted ");
        if figures == 1 {
            text.push_str(&format!("{} ", an(unit)));
        }
        text.push_str(&format!("{unit} within the {name}.  Do you wish to enter?"));
    }
    text
}

/// "a" or "an" for a word, by its first letter (`An`).
pub(crate) fn an(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

/// What the treasure box says (`Lair_Treasure_Popup`): the things found, joined by commas and
/// an "and" before the last, or "absolutely nothing". A single gold piece or mana crystal is not
/// mentioned (the original's test is "more than one").
pub(crate) fn treasure_text(
    loot: &Loot,
    spell: Option<&str>,
    retorts: &[&str],
    item_kinds: &[&str],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if loot.gold > 1 {
        parts.push(format!("{} gold", loot.gold));
    }
    if loot.mana > 1 {
        parts.push(format!("{} mana crystals", loot.mana));
    }
    if loot.prisoner {
        parts.push("a prisoner".to_string());
    }
    if let Some(spell) = spell {
        parts.push(format!("{} {spell} spell", an(spell)));
    }
    let mut realms: Vec<&String> = Vec::new();
    for realm in &loot.books {
        if !realms.contains(&realm) {
            realms.push(realm);
        }
    }
    for realm in realms {
        let n = loot.books.iter().filter(|r| *r == realm).count();
        let name = crate::bigbook::capitalise(realm);
        parts.push(format!(
            "{n} {name} spell book{}",
            if n > 1 { "s" } else { "" }
        ));
    }
    for retort in retorts {
        parts.push(format!("a retort of {retort}"));
    }
    for kind in item_kinds {
        parts.push(format!("{} {kind}", an(kind)));
    }
    let list = match parts.len() {
        0 => "absolutely nothing".to_string(),
        1 => parts.remove(0),
        _ => {
            let last = parts.pop().unwrap_or_default();
            format!("{} and {last}", parts.join(", "))
        }
    };
    format!("Inside you find {list}.")
}

/// The name an item's kind goes by in the box.
fn kind_name(kind: f4x_core::rules::ItemKind) -> &'static str {
    use f4x_core::rules::ItemKind as K;
    match kind {
        K::Sword => "Sword",
        K::Mace => "Mace",
        K::Axe => "Axe",
        K::Bow => "Bow",
        K::Staff => "Staff",
        K::Wand => "Wand",
        K::Misc => "Jewelry",
        K::Shield => "Shield",
        K::Chainmail => "Chainmail",
        K::Platemail => "Platemail",
    }
}

/// Where the box sits and how the text runs round the picture in its corner.
const BOX_X: f32 = 68.0;
const TEXT_WIDTH: f32 = 166.0;
/// The picture corner the first lines keep clear of: 57 wide from the box edge, 55 high.
const PICTURE_CLEAR: (f32, f32) = (49.0, 45.0);

impl App {
    /// The news of a site event, for the log and the boxes.
    pub(crate) fn site_encounter(&mut self, units: Vec<hecs::Entity>, to: Position) {
        let Some((_, lair)) = self.game.economy().lair_at(to) else {
            return;
        };
        let site = site_type(&lair.site);
        let first = lair.guards.first().and_then(|(id, _)| {
            let unit = self.game.content().rules.units.by_name(id)?;
            let figures = unit.stats.get(&f4x_core::rules::Stat::Figures).copied();
            Some((unit.name.clone(), figures.unwrap_or(1) as u32))
        });
        let text = found_text(site, first.as_ref().map(|(name, f)| (name.as_str(), *f)));
        self.site_notices.push_back(SiteNotice::Ask {
            units,
            to,
            text,
            site,
        });
    }

    /// A site was cleared by the human: the box for an unguarded one first, then the treasure.
    pub(crate) fn site_cleared(&mut self, site: &str, guarded: bool, loot: &Loot) {
        let kind = site_type(site);
        if !guarded {
            self.site_notices.push_back(SiteNotice::Found {
                text: found_text(kind, None),
                site: kind,
            });
        }
        let spell = loot
            .spell
            .map(|s| self.game.content().rules.spells.get(s).name.clone());
        let retorts: Vec<&str> = loot
            .retorts
            .iter()
            .map(|&r| f4x_core::wizard::retort_name(r))
            .collect();
        let kinds: Vec<&str> = loot.items.iter().map(|i| kind_name(i.kind)).collect();
        let text = treasure_text(loot, spell.as_deref(), &retorts, &kinds);
        self.site_notices.push_back(SiteNotice::Treasure { text });
        if !loot.items.is_empty() {
            self.item_pool_pending = true;
        }
    }

    /// For the screenshot check: the box of kind `ask`, `found` or `treasure` for the first
    /// guarded site of the map (the treasure is a made-up one).
    pub fn open_test_site(&mut self, kind: &str) {
        let Some((to, lair)) = self.game.map().and_then(|m| {
            m.lairs()
                .iter()
                .find(|l| l.intact && !l.guards.is_empty() && l.site != "tower")
                .map(|l| {
                    (
                        Position {
                            plane: l.plane,
                            x: l.x,
                            y: l.y,
                        },
                        l.clone(),
                    )
                })
        }) else {
            return;
        };
        self.center_on(to);
        let site = site_type(&lair.site);
        match kind {
            "ask" => {
                let first = lair.guards.first().and_then(|(id, _)| {
                    let unit = self.game.content().rules.units.by_name(id)?;
                    Some((unit.name.clone(), 1))
                });
                let text = found_text(site, first.as_ref().map(|(n, f)| (n.as_str(), *f)));
                self.site_notices.push_back(SiteNotice::Ask {
                    units: Vec::new(),
                    to,
                    text,
                    site,
                });
            }
            "found" => self.site_notices.push_back(SiteNotice::Found {
                text: found_text(site, None),
                site,
            }),
            _ => {
                let loot = Loot {
                    gold: 120,
                    mana: 30,
                    prisoner: true,
                    books: vec!["nature".into()],
                    ..Loot::default()
                };
                self.site_cleared(&lair.site, true, &loot);
            }
        }
    }

    /// Show the first waiting box over the map. True while a box took the frame.
    pub(crate) fn site_notice_frame(&mut self) -> bool {
        if self.game.in_battle() || !matches!(self.screen, Screen::Map) {
            return false;
        }
        let Some(notice) = self.site_notices.front().cloned() else {
            // The items awarded wait for the boxes before them, and for the hero, if there is
            // one, to be dealt with first.
            if self.item_pool_pending
                && self.event_notices.is_empty()
                && self.game.offers(crate::view::HUMAN).is_empty()
            {
                if self.item_pool_warning {
                    if let Some(closed) = self.hero_died_warning() {
                        if closed {
                            self.item_pool_warning = false;
                        }
                        return true;
                    }
                    self.item_pool_warning = false;
                }
                self.item_pool_pending = false;
                self.open_item_screen();
                return true;
            }
            return false;
        };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let answer = match &notice {
            SiteNotice::Ask { text, site, .. } => self.confirm_site_box(&lay, text, *site, true),
            SiteNotice::Found { text, site } => self.confirm_site_box(&lay, text, *site, false),
            SiteNotice::Treasure { text } => self.treasure_box(&lay, text),
        };
        let Some(yes) = answer else {
            return true;
        };
        self.site_notices.pop_front();
        if let SiteNotice::Ask { units, to, .. } = notice {
            if yes {
                self.begin_battle(Command::EnterSite { units, to });
            }
        }
        true
    }

    /// `Lair_Confirm`: the box with the site's picture; with buttons it returns the answer
    /// (`Some(true)` for Yes), without any click or key closes it.
    fn confirm_site_box(
        &mut self,
        lay: &Layout,
        text: &str,
        site: usize,
        buttons: bool,
    ) -> Option<bool> {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines = self.wrap_round_picture(text, line);
        let paragraph = line * lines.len() as f32;
        let top_y = ((200.0 - (34.0 + paragraph)) / 2.0).floor();
        let bottom = paragraph.max(75.0) + top_y + 6.0 + 6.0;
        self.draw_pic_rows(lay, "backgrnd", 25, BOX_X, top_y, top_y, bottom);
        self.draw_pic(lay, "reload", 9 + site, 0, BOX_X + 7.0, top_y + 7.0);
        self.draw_pic(
            lay,
            "backgrnd",
            if buttons { 26 } else { 27 },
            0,
            BOX_X,
            bottom,
        );
        self.draw_box_text(view, &lines, BOX_X + 9.0, top_y + 10.0, line);
        let mouse = Vec2::from(mouse_position());
        let down = is_mouse_button_down(MouseButton::Left);
        if buttons {
            let yes_at = (BOX_X + 100.0, bottom + 5.0);
            let no_at = (BOX_X + 17.0, bottom + 5.0);
            let yes = lay.rect((yes_at.0, yes_at.1, 33.0, 17.0));
            let no = lay.rect((no_at.0, no_at.1, 33.0, 17.0));
            self.draw_pic(
                lay,
                "resource",
                3,
                usize::from(down && yes.contains(mouse)),
                yes_at.0,
                yes_at.1,
            );
            self.draw_pic(
                lay,
                "resource",
                4,
                usize::from(down && no.contains(mouse)),
                no_at.0,
                no_at.1,
            );
            let click = is_mouse_button_pressed(MouseButton::Left);
            if (click && yes.contains(mouse)) || is_key_pressed(KeyCode::Y) {
                return Some(true);
            }
            if (click && no.contains(mouse))
                || is_key_pressed(KeyCode::N)
                || is_key_pressed(KeyCode::Escape)
            {
                return Some(false);
            }
            None
        } else if is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Space)
        {
            Some(true)
        } else {
            None
        }
    }

    /// `Lair_Treasure_Popup`'s `Notify1` box: a strip of the notify art as wide as the widest
    /// line, the picture box at its right end holding the treasure picture. A click or a key
    /// closes it.
    fn treasure_box(&mut self, lay: &Layout, text: &str) -> Option<bool> {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines = self.bm_wrap(4, text, 175.0);
        let widest = lines
            .iter()
            .map(|l| self.art.text_width(4, l))
            .fold(0.0f32, f32::max)
            .ceil();
        let x = (190.0 - ((96.0 + widest) / 2.0)).floor();
        let y = 60.0;
        self.clipped((0.0, 0.0, widest + x + 12.0, 200.0), |app| {
            app.draw_pic(lay, "resource", 56, 0, x, y);
        });
        let right = x + widest + 12.0;
        self.draw_pic(lay, "resource", 57, 0, right, y);
        self.draw_pic(lay, "reload", 20, 0, right + 2.0 + 4.0, y + 8.0);
        let lines: Vec<(f32, String)> = lines.into_iter().map(|l| (0.0, l)).collect();
        self.draw_box_text(view, &lines, x + 10.0, y + 8.0, line);
        (is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Space))
        .then_some(true)
    }

    /// The text of a box: a dark shadow a pixel right and below, then the light text.
    pub(crate) fn draw_box_text(
        &mut self,
        view: (f32, f32, f32),
        lines: &[(f32, String)],
        x: f32,
        y: f32,
        line: f32,
    ) {
        let shadow = Color::new(0.05, 0.1, 0.2, 1.0);
        let light = Color::new(0.97, 0.95, 0.85, 1.0);
        for (i, (indent, text)) in lines.iter().enumerate() {
            let at = y + line * i as f32;
            self.bm_text(
                view,
                4,
                shadow,
                text,
                x + indent + 1.0,
                at + 1.0,
                Align::Left,
            );
            self.bm_text(view, 4, light, text, x + indent, at, Align::Left);
        }
    }

    /// The words of `text` in lines of the box's width, the first ones narrower so they keep
    /// clear of the picture in the corner. Each line comes with its indent.
    fn wrap_round_picture(&self, text: &str, line: f32) -> Vec<(f32, String)> {
        let mut lines: Vec<(f32, String)> = Vec::new();
        let mut current = String::new();
        let geometry = |row: usize| {
            if line * row as f32 + 1.0 < PICTURE_CLEAR.1 + 10.0 {
                (PICTURE_CLEAR.0, TEXT_WIDTH - PICTURE_CLEAR.0)
            } else {
                (0.0, TEXT_WIDTH)
            }
        };
        for word in text.split_whitespace() {
            let attempt = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            let (_, width) = geometry(lines.len());
            if !current.is_empty() && self.art.text_width(4, &attempt) > width {
                let (indent, _) = geometry(lines.len());
                lines.push((indent, std::mem::take(&mut current)));
                current = word.to_string();
            } else {
                current = attempt;
            }
        }
        if !current.is_empty() {
            let (indent, _) = geometry(lines.len());
            lines.push((indent, current));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loot() -> Loot {
        Loot::default()
    }

    #[test]
    fn the_question_names_the_site_and_the_scouts_report() {
        assert_eq!(
            found_text(site_type("mysterious_cave"), Some(("Efreet", 1))),
            "You have found a mysterious cave.  Scouts have spotted an Efreet within the mysterious cave.  Do you wish to enter?"
        );
        assert_eq!(
            found_text(site_type("dungeon"), Some(("Skeletons", 6))),
            "You have found a dungeon.  Scouts have spotted Skeletons within the dungeon.  Do you wish to enter?"
        );
        assert_eq!(
            found_text(site_type("ruins"), None),
            "You have found some ruins."
        );
        assert_eq!(
            found_text(site_type("ancient_temple"), None),
            "You have found an ancient temple."
        );
    }

    #[test]
    fn the_treasure_lists_what_was_found() {
        let mut l = loot();
        assert_eq!(
            treasure_text(&l, None, &[], &[]),
            "Inside you find absolutely nothing."
        );
        l.gold = 120;
        assert_eq!(
            treasure_text(&l, None, &[], &[]),
            "Inside you find 120 gold."
        );
        l.mana = 30;
        l.prisoner = true;
        assert_eq!(
            treasure_text(&l, Some("Earth Lore"), &[], &["Sword"]),
            "Inside you find 120 gold, 30 mana crystals, a prisoner, an Earth Lore spell and a Sword."
        );
        l.gold = 1;
        l.mana = 0;
        l.prisoner = false;
        l.books = vec!["nature".into(), "nature".into()];
        assert_eq!(
            treasure_text(&l, None, &["Sage Master"], &[]),
            "Inside you find 2 Nature spell books and a retort of Sage Master."
        );
    }
}
