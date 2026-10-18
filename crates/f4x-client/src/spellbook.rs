//! The spell book: the small book the wizard casts from (`Spellbook_Screen`, `SBookScr.c`),
//! drawn on the original's two-page picture. Known spells are grouped by realm, six to a page,
//! each with its mana cost and a row of the realm's symbol for the turns it takes (or "Instant
//! Cast"). Clicking a spell starts casting it; one that needs a target is aimed on the map.
//! The big book (the library and research) is in `bigbook.rs`.

use f4x_core::rules::exec::Target;
use f4x_core::rules::{CastContext, SpellId, SpellTarget};
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// A spell waiting for the player to click its target on the map.
#[derive(Clone, Copy)]
pub(crate) struct Targeting {
    pub spell: SpellId,
    /// The spell is already paid for (cast over several turns) rather than starting now.
    pub resolve: bool,
}

/// One page of a book: its title and the spells on it.
#[derive(Debug, Clone, Default)]
pub(crate) struct BookPage {
    pub title: String,
    pub spells: Vec<SpellId>,
}

/// The small book's state.
#[derive(Debug, Clone, Default)]
pub(crate) struct BookUi {
    /// The pages, always an even number.
    pub pages: Vec<BookPage>,
    /// The left page of the spread shown.
    pub left: usize,
    /// Where the player last left off (the book opens there again).
    pub last_left: usize,
    /// A spell to start once the player has agreed to give up the one being cast, and whether
    /// giving it up is all that was asked (the spell clicked was the one being cast).
    pub replace: Option<(SpellId, bool)>,
    /// The combat book (`Combat_Spellbook_Screen`): how each spell stands for the caster, in
    /// place of the overland entries. `None` for the overland book.
    pub combat: Option<std::collections::HashMap<SpellId, CombatEntry>>,
}

/// A spell in the combat book (`Combat_Spellbook_Compose`): what it costs now and how many
/// times the caster could pay it this turn, shown as that many realm symbols (none: greyed).
#[derive(Debug, Clone, Copy)]
pub(crate) struct CombatEntry {
    pub cost: i32,
    pub extra: bool,
    pub icons: usize,
}

/// The realms in the order of the book's pages.
const BOOK_REALMS: [&str; 6] = ["arcane", "chaos", "death", "life", "nature", "sorcery"];

/// Spells per page of the small book and of the big one.
pub(crate) const SMALL_PAGE: usize = 6;

impl App {
    /// Pages of spells, `per_page` to a page, grouped by realm in the book's order, each
    /// group on its own pages; the last page is padded so the book has an even number.
    pub(crate) fn book_pages(&self, spells: &[SpellId], per_page: usize) -> Vec<BookPage> {
        let rules = &self.game.content().rules;
        let mut pages: Vec<BookPage> = Vec::new();
        for realm in BOOK_REALMS {
            let mut of: Vec<SpellId> = spells
                .iter()
                .copied()
                .filter(|&s| rules.spells.get(s).realm == realm)
                .collect();
            of.sort();
            let title = rules
                .realms
                .by_name(realm)
                .map_or(realm, |r| r.name.as_str());
            for chunk in of.chunks(per_page) {
                pages.push(BookPage {
                    title: title.to_string(),
                    spells: chunk.to_vec(),
                });
            }
        }
        if pages.len() % 2 == 1 {
            pages.push(BookPage::default());
        }
        pages
    }

    /// The spells the spell book offers: the known ones that can be cast on the map.
    fn castable_spells(&self) -> Vec<SpellId> {
        let Some(m) = self.game.magic(HUMAN) else {
            return Vec::new();
        };
        m.known
            .iter()
            .copied()
            .filter(|&s| {
                self.game
                    .content()
                    .rules
                    .spells
                    .get(s)
                    .contexts
                    .contains(&CastContext::Overland)
            })
            .collect()
    }

    pub fn open_spellbook(&mut self) {
        let spells = self.castable_spells();
        let pages = self.book_pages(&spells, SMALL_PAGE);
        // Open at the spell being cast, or where the book was left.
        let mut left = self.book_ui.last_left.min(pages.len().saturating_sub(2));
        if let Some(c) = self.game.magic(HUMAN).and_then(|m| m.casting) {
            if let Some(i) = pages.iter().position(|p| p.spells.contains(&c.spell)) {
                left = i - i % 2;
            }
        }
        self.book_ui.pages = pages;
        self.book_ui.left = left - left % 2;
        self.book_ui.replace = None;
        self.book_ui.combat = None;
        self.screen = Screen::Spellbook;
    }

    /// Start casting `spell`: one that needs a target and is paid for at once is aimed on the
    /// map, any other is started and paid for over the turns.
    pub(crate) fn begin_cast(&mut self, spell: SpellId) {
        let needs = self.game.spell_needs_target(HUMAN, spell);
        let cost = self.game.casting_cost(HUMAN, spell, false);
        let instant = self.game.magic(HUMAN).map_or(0, |m| m.skill_left);
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        if needs && cost <= instant.min(mana) {
            self.targeting = Some(Targeting {
                spell,
                resolve: false,
            });
            self.screen = Screen::Map;
            return;
        }
        let events = self.game.apply(Command::CastSpell {
            player: HUMAN,
            spell,
            target: None,
        });
        self.note(events);
        self.screen = Screen::Map;
    }

    pub(crate) fn spellbook_screen(&mut self) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        // A question first: give up the spell being cast for another?
        if let Some((spell, abort_only)) = self.book_ui.replace {
            clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
            self.draw_spellbook(&lay, view);
            let casting = self
                .game
                .magic(HUMAN)
                .and_then(|m| m.casting)
                .map(|c| self.game.content().rules.spells.get(c.spell).name.clone())
                .unwrap_or_default();
            match self.confirm_box(&format!("Do you wish to abort the {casting} spell?")) {
                Some(true) => {
                    self.book_ui.replace = None;
                    let events = self.game.apply(Command::CancelCast { player: HUMAN });
                    self.note(events);
                    if !abort_only && self.game.magic(HUMAN).is_some_and(|m| m.casting.is_none()) {
                        self.begin_cast(spell);
                    }
                }
                Some(false) => self.book_ui.replace = None,
                None => {}
            }
            return;
        }
        if self.book_turn.is_some() && !self.book_turn_frame() {
            return;
        }
        let pages = self.book_ui.pages.len();
        let left = self.book_ui.left;
        // Page turns: the corners, the arrow keys and the wheel.
        let (_, wheel) = mouse_wheel();
        if (is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::F) || wheel < 0.0)
            && left + 2 < pages
        {
            self.turn_book_page(false, true);
            return;
        }
        if (is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::B) || wheel > 0.0) && left >= 2
        {
            self.turn_book_page(false, false);
            return;
        }
        let in_rect = |x: f32, y: f32, w: f32, h: f32| {
            lay.rect((x, y, w, h)).contains(mouse_position().into())
        };
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        if clicked && in_rect(29.0, 14.0, 14.0, 13.0) && left >= 2 {
            self.turn_book_page(false, false);
            return;
        }
        if clicked && in_rect(274.0, 14.0, 14.0, 13.0) && left + 2 < pages {
            self.turn_book_page(false, true);
            return;
        }
        // The ribbon at the bottom, Escape, V or a click outside the book close it.
        let ribbon = clicked && in_rect(175.0, 166.0, 19.0, 30.0);
        if ribbon || is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::V) {
            self.book_ui.last_left = self.book_ui.left;
            self.screen = Screen::Map;
            return;
        }
        // Clicking a spell.
        let left = self.book_ui.left;
        for (side, page_i) in [(0usize, left), (1, left + 1)] {
            let Some(page) = self.book_ui.pages.get(page_i) else {
                continue;
            };
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            for (i, &spell) in page.spells.iter().enumerate() {
                if clicked && in_rect(x0, 29.0 + 22.0 * i as f32, 121.0, 17.0) {
                    self.book_ui.last_left = left;
                    self.choose_spell_to_cast(spell);
                    return;
                }
            }
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_spellbook(&lay, view);
    }

    /// The player picked a spell in the book: cast it, asking first if another is under way.
    fn choose_spell_to_cast(&mut self, spell: SpellId) {
        let casting = self.game.magic(HUMAN).and_then(|m| m.casting);
        match casting {
            // The spell being cast: only an offer to give it up.
            Some(c) if c.spell == spell => self.book_ui.replace = Some((spell, true)),
            Some(_) => self.book_ui.replace = Some((spell, false)),
            None => self.begin_cast(spell),
        }
    }

    /// The open book: the page spread, its titles and spells, the corners and the ribbon.
    pub(crate) fn draw_spellbook(&mut self, lay: &Layout, view: (f32, f32, f32)) {
        let (pages, left) = (self.book_ui.pages.len(), self.book_ui.left);
        self.draw_book_spread(lay, view, left, left + 1, (left >= 2, left + 2 < pages));
    }

    /// The book picture with the pages `left_i` and `right_i` on it and the dog-ears `corners`
    /// (left, right) asked for: a page turn shows other pages than the book's current spread.
    pub(crate) fn draw_book_spread(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        left_i: usize,
        right_i: usize,
        corners: (bool, bool),
    ) {
        // The combat book lies on the battlefield; the overland one on a dark ground.
        if self.book_ui.combat.is_none() {
            let full = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.1, 0.08, 0.06, 1.0),
            );
        }
        if self.draw_pic(lay, "spells", 0, 0, 16.0, 10.0).is_none() {
            let r = lay.rect((16.0, 10.0, 285.0, 186.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.55, 0.4, 0.2, 1.0));
        }
        if corners.0 {
            self.draw_pic(lay, "spells", 1, 0, 29.0, 14.0);
        }
        if corners.1 {
            self.draw_pic(lay, "spells", 2, 0, 274.0, 14.0);
        }
        for (side, page_i) in [(0usize, left_i), (1, right_i)] {
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            self.draw_book_page(lay, view, page_i, side == 1, x0, (x0 + 54.0, 16.0), 0.0);
        }
    }

    /// A page of the small book: its title (on the left page and on a right page that starts a
    /// realm) centred at `title`, and its spells with the page's left edge at `x0` and `dy`
    /// added to their y.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_book_page(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        page_i: usize,
        right: bool,
        x0: f32,
        title: (f32, f32),
        dy: f32,
    ) {
        let Some(page) = self.book_ui.pages.get(page_i).cloned() else {
            return;
        };
        let starts_realm = page_i == 0
            || self
                .book_ui
                .pages
                .get(page_i - 1)
                .is_none_or(|p| p.title != page.title);
        if !right || starts_realm {
            self.bm_text(
                view,
                4,
                Color::new(0.45, 0.2, 0.1, 1.0),
                &page.title,
                title.0,
                title.1,
                Align::Center,
            );
        }
        for (i, &spell) in page.spells.iter().enumerate() {
            self.draw_book_entry(lay, view, spell, x0, 31.0 + 22.0 * i as f32 + dy);
        }
    }

    /// One spell on a page: name and cost on a line, the turns it takes under them.
    fn draw_book_entry(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        spell: SpellId,
        x: f32,
        y: f32,
    ) {
        let def = self.game.content().rules.spells.get(spell).clone();
        if let Some(entry) = self
            .book_ui
            .combat
            .as_ref()
            .and_then(|m| m.get(&spell))
            .copied()
        {
            self.draw_combat_entry(lay, view, &def.name, &def.realm, entry, x, y);
            return;
        }
        let magic = self.game.magic(HUMAN).cloned().unwrap_or_default();
        let casting = magic.casting.filter(|c| c.spell == spell);
        let cost = casting.map_or_else(
            || self.game.casting_cost(HUMAN, spell, false),
            |c| c.remaining,
        );
        let ink = Color::new(0.2, 0.12, 0.05, 1.0);
        // The spell being cast glows (palette index 198 cycling, `SmlBook_Draw`).
        let hot = crate::cycle::SMALL_BOOK.colour(get_time());
        let color = if casting.is_some() { hot } else { ink };
        self.bm_text(view, 1, color, &def.name, x, y, Align::Left);
        self.bm_text(
            view,
            1,
            color,
            &format!("{} MP", cost.max(0)),
            x + 121.0,
            y,
            Align::Right,
        );
        // Turns to cast: a symbol for each, twenty to a row; no symbols for an instant spell.
        let skill = self.game.magic(HUMAN).map_or(0, |m| m.casting_skill());
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let income = self.game.income(HUMAN).split.mana;
        let limit = skill.min(mana + income).max(0);
        let turns = if limit == 0 {
            40
        } else {
            (cost / limit).min(40)
        };
        let realm_symbol = ["nature", "sorcery", "chaos", "life", "death", "arcane"]
            .iter()
            .position(|r| *r == def.realm)
            .map_or(9, |i| 4 + i);
        if turns < 1 {
            let instant = magic.skill_left.min(mana) >= cost;
            let crafting = matches!(def.id.as_str(), "enchant_item" | "create_artifact");
            if instant && !crafting {
                self.bm_text(view, 1, ink, "Instant Cast", x + 9.0, y + 7.0, Align::Left);
            } else if instant {
                self.bm_text(view, 1, ink, "Special", x + 9.0, y + 7.0, Align::Left);
            } else {
                self.draw_pic(lay, "spells", realm_symbol, 0, x + 1.0, y + 7.0);
            }
        } else {
            for t in 0..turns {
                let (col, row) = (t % 20, t / 20);
                self.draw_pic(
                    lay,
                    "spells",
                    realm_symbol,
                    0,
                    x + 1.0 + 6.0 * col as f32,
                    y + 7.0 + 6.0 * row as f32,
                );
            }
        }
    }

    /// One spell of the combat book (`Combat_Spellbook_Compose`): the name and what it costs
    /// now, and a realm symbol for each time the caster could pay for it this turn (twenty to
    /// a row); a spell that cannot be cast now is greyed.
    #[allow(clippy::too_many_arguments)]
    fn draw_combat_entry(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        name: &str,
        realm: &str,
        entry: CombatEntry,
        x: f32,
        y: f32,
    ) {
        let ink = Color::new(0.2, 0.12, 0.05, 1.0);
        let grey = Color::new(0.55, 0.5, 0.45, 1.0);
        let color = if entry.icons == 0 { grey } else { ink };
        self.bm_text(view, 1, color, name, x, y, Align::Left);
        let cost = if entry.cost == 0 {
            "?".to_string()
        } else {
            format!("{}{} MP", entry.cost, if entry.extra { "+" } else { "" })
        };
        self.bm_text(view, 1, color, &cost, x + 121.0, y, Align::Right);
        let symbol = ["nature", "sorcery", "chaos", "life", "death", "arcane"]
            .iter()
            .position(|r| *r == realm)
            .map_or(9, |i| 4 + i);
        for t in 0..entry.icons.min(40) {
            let (col, row) = (t % 20, t / 20);
            self.draw_pic(
                lay,
                "spells",
                symbol,
                0,
                x + 1.0 + 6.0 * col as f32,
                y + 7.0 + 6.0 * row as f32,
            );
        }
    }

    /// The player clicked `pos` while choosing a target.
    pub(crate) fn aim_at(&mut self, t: Targeting, pos: f4x_core::city::Position) {
        let def = self.game.content().rules.spells.get(t.spell);
        let target = match def.target {
            SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit => {
                let units = self.game.units().units_at(pos);
                let wanted = units.into_iter().find(|&u| {
                    let mine = self.game.units().unit_owner(u) == Some(HUMAN);
                    match def.target {
                        SpellTarget::OwnUnit => mine,
                        SpellTarget::EnemyUnit => !mine,
                        _ => true,
                    }
                });
                wanted.map(Target::Unit)
            }
            SpellTarget::OwnCity | SpellTarget::EnemyCity => {
                self.game.economy().city_at(pos).map(Target::City)
            }
            // Another wizard, by clicking one of their cities or units.
            SpellTarget::EnemyWizard => self
                .game
                .economy()
                .city_at(pos)
                .and_then(|c| self.game.economy().city_owner(c))
                .or_else(|| {
                    self.game
                        .units()
                        .units_at(pos)
                        .into_iter()
                        .find_map(|u| self.game.units().unit_owner(u))
                })
                .filter(|&o| o != HUMAN)
                .map(Target::Player),
            _ => Some(Target::Tile(pos)),
        };
        // A Spell Ward also needs a realm: ask for it.
        if self
            .game
            .spell_has_hook(t.spell, f4x_core::city_spells::SPELL_WARD)
        {
            match target {
                Some(Target::City(city))
                    if self.game.economy().city_owner(city) == Some(HUMAN)
                        && !self.game.fully_warded(city) =>
                {
                    self.ward_prompt = Some((t.spell, city, t.resolve));
                }
                _ => {
                    self.log.push(
                        "Choose one of your cities that is not warded against every realm".into(),
                    );
                    self.targeting = Some(t);
                }
            }
            return;
        }
        let Some(target) = target else {
            self.log.push("No valid target there".into());
            self.targeting = Some(t);
            return;
        };
        let cmd = if t.resolve {
            Command::ResolveCast {
                player: HUMAN,
                target,
            }
        } else {
            Command::CastSpell {
                player: HUMAN,
                spell: t.spell,
                target: Some(target),
            }
        };
        let events = self.game.apply(cmd);
        let rejected = events
            .iter()
            .any(|e| matches!(e, f4x_core::Event::Rejected(_)));
        self.note(events);
        if rejected {
            self.targeting = Some(t);
        }
    }
}
