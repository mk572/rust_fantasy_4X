//! The spell book: the small book the wizard casts from (`Spellbook_Screen`, `SBookScr.c`),
//! drawn on the original's two-page picture. Known spells are grouped by realm, six to a page,
//! each with its mana cost and a row of the realm's symbol for the turns it takes (or "Instant
//! Cast"). Clicking a spell starts casting it; one that needs a target is aimed on the map.
//! The big book (the library and research) is in `bigbook.rs`.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::page_turn::{Book, BookTurn};
use crate::ui;
use f4x_core::game::Command;
use f4x_core::rules::exec::Target;
use f4x_core::rules::{CastContext, SpellId, SpellTarget};
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
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

/// Which spells the map's spell book lists (a click on the line at the foot of the left page).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BookFilter {
    /// Every spell of the game; the ones the wizard does not know are dimmed.
    All,
    /// Every known spell; the battle ones cannot be cast on the map and are dimmed.
    Known,
    /// The known spells that can be cast on the map.
    #[default]
    KnownOverland,
    /// The known spells that can be cast in battle (dimmed: they are cast from the combat book).
    KnownBattle,
}

impl BookFilter {
    const ALL: [BookFilter; 4] = [
        BookFilter::All,
        BookFilter::Known,
        BookFilter::KnownOverland,
        BookFilter::KnownBattle,
    ];

    fn next(self) -> Self {
        let i = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    fn text_key(self) -> &'static str {
        match self {
            BookFilter::All => "spell.show_all",
            BookFilter::Known => "spell.show_known",
            BookFilter::KnownOverland => "spell.show_overland",
            BookFilter::KnownBattle => "spell.show_battle",
        }
    }
}

/// How the spells of each realm are ordered in the map's spell book.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BookSort {
    #[default]
    Name,
    ManaCost,
}

impl BookSort {
    fn next(self) -> Self {
        match self {
            BookSort::Name => BookSort::ManaCost,
            BookSort::ManaCost => BookSort::Name,
        }
    }

    fn text_key(self) -> &'static str {
        match self {
            BookSort::Name => "spell.sort_name",
            BookSort::ManaCost => "spell.sort_cost",
        }
    }
}

/// What a click on a spell the wizard does not know asks or says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BookAsk {
    /// "Learn this spell next?": research it (or queue it behind the research under way).
    Learn(SpellId),
    /// A message with OK: the text key.
    Say(&'static str),
}

/// The small book's state.
#[derive(Debug, Clone, Default)]
pub(crate) struct BookUi {
    /// How the spells of a realm are ordered.
    pub sort: BookSort,
    /// A question or message over the book.
    pub ask: Option<BookAsk>,
    /// Which spells the map book lists.
    pub filter: BookFilter,
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
    /// The page turn in progress.
    pub turn: Option<BookTurn>,
}

/// What the small book keeps between openings: its order, its filter and where it was left.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BookPrefs {
    pub sort: BookSort,
    pub filter: BookFilter,
    pub last_left: usize,
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
        self.book_pages_sorted(spells, per_page, None)
    }

    /// `book_pages`, with each realm's spells in the order `sort` gives (none: the rules').
    pub(crate) fn book_pages_sorted(
        &self,
        spells: &[SpellId],
        per_page: usize,
        sort: Option<BookSort>,
    ) -> Vec<BookPage> {
        let rules = &self.game.content().rules;
        let mut pages: Vec<BookPage> = Vec::new();
        for realm in BOOK_REALMS {
            let mut of: Vec<SpellId> = spells
                .iter()
                .copied()
                .filter(|&s| rules.spells.get(s).realm == realm)
                .collect();
            match sort {
                None => of.sort(),
                Some(BookSort::Name) => of.sort_by_key(|&s| rules.spells.get(s).name.clone()),
                Some(BookSort::ManaCost) => of.sort_by_key(|&s| {
                    (
                        self.game.casting_cost(HUMAN, s, false),
                        rules.spells.get(s).name.clone(),
                    )
                }),
            }
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

    /// Whether a click on `spell` in the map's book starts casting it: the wizard knows it and
    /// it can be cast on the map.
    fn book_spell_usable(&self, spell: SpellId) -> bool {
        self.game.wizard(HUMAN).is_some_and(|m| m.knows(spell))
            && self
                .game
                .content()
                .rules
                .spells
                .get(spell)
                .contexts
                .contains(&CastContext::Overland)
    }

    /// What a click on a spell the wizard does not know leads to: an offer to learn it next when
    /// it is on offer for research, else the reason it cannot be learnt.
    fn why_not_learnable(&self, spell: SpellId) -> BookAsk {
        let Some(m) = self.game.wizard(HUMAN) else {
            return BookAsk::Say("spell.cannot_learn_yet");
        };
        let realm = self.game.content().rules.spells.get(spell).realm.clone();
        // Arcane spells need no book; every other realm does.
        if realm != "arcane" && m.books().get(&realm).copied().unwrap_or(0) == 0 {
            return BookAsk::Say("spell.lack_books");
        }
        if m.candidates().contains(&spell) {
            BookAsk::Learn(spell)
        } else {
            BookAsk::Say("spell.cannot_learn_yet")
        }
    }

    /// A message with OK; true once the player has read it.
    fn info_box(&mut self, text: &str, input: &Input) -> bool {
        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (w.min(560.0), 130.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, crate::ui::GOLD);
        crate::frame::text_centered(text, w / 2.0, y + 45.0, 22.0, WHITE);
        let ok = crate::ui::button(
            Rect::new(x + bw / 2.0 - 50.0, y + 70.0, 100.0, 36.0),
            &self.text("ui.ok"),
            true,
            input,
        );
        ok || input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Space)
    }

    /// The spell the wizard has paid for over turns is ready: choose its target. A rival's
    /// global enchantment is picked from a list, anything else is aimed on the map.
    pub(crate) fn aim_paid_spell(&mut self, spell: SpellId) {
        self.screen = Screen::Map;
        if self
            .game
            .spell_has_hook(spell, f4x_core::spell::hero::RESURRECTION)
        {
            self.open_resurrect(spell, true);
            return;
        }
        if self.game.content().rules.spells.get(spell).target == SpellTarget::GlobalEnchantment {
            if self.game.rival_globals(HUMAN).is_empty() {
                self.log.push(self.text("spell.no_globals_waits"));
            } else {
                self.modals.open(crate::view::Global {
                    spell,
                    resolve: true,
                    row: 0,
                });
            }
            return;
        }
        self.targeting = Some(Targeting {
            spell,
            resolve: true,
        });
    }

    /// Start casting `spell`: one that needs a target and is paid for at once is aimed on the
    /// map, any other is started and paid for over the turns.
    pub(crate) fn begin_cast(&mut self, spell: SpellId) {
        // The two spells that make items are designed first.
        let id = self.game.content().rules.spells.get(spell).id.clone();
        if let Some(craft) = f4x_core::item::crafting::Craft::from_spell(&id) {
            self.open_craft(craft);
            return;
        }
        let needs = self.game.spell_needs_target(HUMAN, spell);
        let cost = self.game.casting_cost(HUMAN, spell, false);
        let instant = self.game.wizard(HUMAN).map_or(0, |m| m.skill_left());
        let mana = self.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        if needs && cost <= instant.min(mana) {
            // The dead hero Resurrection raises is chosen from a list.
            if self
                .game
                .spell_has_hook(spell, f4x_core::spell::hero::RESURRECTION)
            {
                self.open_resurrect(spell, false);
                return;
            }
            // A rival's global enchantment is no place on the map: it is chosen from a list.
            if self.game.content().rules.spells.get(spell).target == SpellTarget::GlobalEnchantment
            {
                if self.game.rival_globals(HUMAN).is_empty() {
                    self.log.push(self.text("spell.no_globals"));
                } else {
                    self.modals.open(crate::view::Global {
                        spell,
                        resolve: false,
                        row: 0,
                    });
                }
                self.screen = Screen::Map;
                return;
            }
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
        self.draw_styled(view, &Style::nearest(1, color), name, x, y);
        let cost = if entry.cost == 0 {
            "?".to_string()
        } else {
            format!("{}{} MP", entry.cost, if entry.extra { "+" } else { "" })
        };
        self.draw_styled(view, &Style::nearest(1, color).right(), &cost, x + 121.0, y);
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
    pub(crate) fn aim_at(&mut self, t: Targeting, pos: f4x_core::map::Position) {
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
            .spell_has_hook(t.spell, f4x_core::spell::city::SPELL_WARD)
        {
            match target {
                Some(Target::City(city))
                    if self.game.economy().city_owner(city) == Some(HUMAN)
                        && !self.game.fully_warded(city) =>
                {
                    self.modals.open(crate::view::Ward {
                        spell: t.spell,
                        city,
                        resolve: t.resolve,
                    });
                }
                _ => {
                    self.log.push(self.text("spell.choose_unwarded"));
                    self.targeting = Some(t);
                }
            }
            return;
        }
        let Some(target) = target else {
            self.log.push(self.text("spell.no_valid_target"));
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
            .any(|e| matches!(e, f4x_core::game::Event::Rejected(_)));
        self.note(events);
        if rejected {
            self.targeting = Some(t);
        }
    }

    /// For the screenshot check: set the filter and the sort (`all`, `known`, `overland`,
    /// `battle`, with `,cost` for the order by mana cost).
    pub fn test_book_view(&mut self, view: &str) {
        let (filter, sort) = view.split_once(',').unwrap_or((view, "name"));
        self.book_prefs.filter = match filter {
            "all" => BookFilter::All,
            "known" => BookFilter::Known,
            "battle" => BookFilter::KnownBattle,
            _ => BookFilter::KnownOverland,
        };
        self.book_prefs.sort = if sort == "cost" {
            BookSort::ManaCost
        } else {
            BookSort::Name
        };
    }

    pub fn open_spellbook(&mut self) {
        if self.refuse_banished_spells() {
            return;
        }
        // A spell that is paid for and waits for its target: choose that first.
        if let Some(spell) = self.game.wizard(HUMAN).and_then(|m| m.casting_paid()) {
            if self.game.spell_needs_target(HUMAN, spell) {
                self.aim_paid_spell(spell);
                return;
            }
        }
        let mut ui = BookUi {
            sort: self.book_prefs.sort,
            filter: self.book_prefs.filter,
            last_left: self.book_prefs.last_left,
            ..Default::default()
        };
        let spells = ui.castable_spells(self);
        ui.pages = self.book_pages_sorted(&spells, SMALL_PAGE, Some(ui.sort));
        // Open at the spell being cast, or where the book was left.
        let mut left = ui.last_left.min(ui.pages.len().saturating_sub(2));
        if let Some(c) = self.game.wizard(HUMAN).and_then(|m| m.casting()) {
            if let Some(i) = ui.pages.iter().position(|p| p.spells.contains(&c.spell)) {
                left = i - i % 2;
            }
        }
        ui.left = left - left % 2;
        self.open_page(Screen::Spellbook, ui);
    }
}

impl BookUi {
    /// The spells the map's spell book lists, by its filter.
    fn castable_spells(&self, app: &App) -> Vec<SpellId> {
        let Some(m) = app.game.wizard(HUMAN) else {
            return Vec::new();
        };
        let spells = &app.game.content().rules.spells;
        let works_in = |s: SpellId, context| spells.get(s).contexts.contains(&context);
        match self.filter {
            BookFilter::All => spells.iter().map(|(id, _)| id).collect(),
            BookFilter::Known => m.known().iter().copied().collect(),
            BookFilter::KnownOverland => m
                .known()
                .iter()
                .copied()
                .filter(|&s| works_in(s, CastContext::Overland))
                .collect(),
            BookFilter::KnownBattle => m
                .known()
                .iter()
                .copied()
                .filter(|&s| works_in(s, CastContext::Battle))
                .collect(),
        }
    }

    /// The question or message over the book, and what its answer does.
    fn answer_book_ask(&mut self, app: &mut App, ask: BookAsk, input: &Input) {
        match ask {
            BookAsk::Say(key) => {
                let text = app.text(key);
                if app.info_box(&text, input) {
                    self.ask = None;
                }
            }
            BookAsk::Learn(spell) => {
                let name = app.game.content().rules.spells.get(spell).name.clone();
                let researching = app
                    .game
                    .wizard(HUMAN)
                    .and_then(|m| m.researching())
                    .is_some();
                let text = app.text_with(
                    if researching {
                        "spell.learn_next_queue"
                    } else {
                        "spell.learn_next"
                    },
                    &Args::new().text("spell", &name),
                );
                match app.confirm_box(&text, input) {
                    Some(true) => {
                        self.ask = None;
                        if researching {
                            app.queued_research = Some(spell);
                            app.log.push(
                                app.text_with("spell.queued", &Args::new().text("spell", &name)),
                            );
                        } else {
                            let events = app.game.apply(Command::SetResearch {
                                player: HUMAN,
                                spell,
                            });
                            app.note(events);
                        }
                    }
                    Some(false) => self.ask = None,
                    None => {}
                }
            }
        }
    }

    /// Rebuild the pages after the filter changed, and open the book at its first spread.
    fn refilter_book(&mut self, app: &mut App) {
        let spells = self.castable_spells(app);
        self.pages = app.book_pages_sorted(&spells, SMALL_PAGE, Some(self.sort));
        self.left = 0;
        self.last_left = 0;
    }

    /// The spell listed at `(x, y)` (320 by 200 coordinates) of the open book, with its area.
    pub(crate) fn book_spell_at(&self, x: f32, y: f32) -> Option<(SpellId, (f32, f32, f32, f32))> {
        let left = self.left;
        for (side, page_i) in [(0usize, left), (1, left + 1)] {
            let page = self.pages.get(page_i)?;
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            for (i, &spell) in page.spells.iter().enumerate() {
                let area = (x0, 29.0 + 22.0 * i as f32, 121.0, 17.0);
                if x >= area.0 && x < area.0 + area.2 && y >= area.1 && y < area.1 + area.3 {
                    return Some((spell, area));
                }
            }
        }
        None
    }

    /// The player picked a spell in the book: cast it, asking first if another is under way.
    fn choose_spell_to_cast(&mut self, app: &mut App, spell: SpellId) {
        let casting = app.game.wizard(HUMAN).and_then(|m| m.casting());
        match casting {
            // The spell being cast: only an offer to give it up.
            Some(c) if c.spell == spell => self.replace = Some((spell, true)),
            Some(_) => self.replace = Some((spell, false)),
            None => app.begin_cast(spell),
        }
    }

    /// The open book: the page spread, its titles and spells, the corners and the ribbon.
    pub(crate) fn draw_spellbook(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        input: &Input,
    ) {
        let (pages, left) = (self.pages.len(), self.left);
        self.draw_book_spread(
            app,
            lay,
            view,
            left,
            left + 1,
            (left >= 2, left + 2 < pages),
            input,
        );
    }

    /// The book picture with the pages `left_i` and `right_i` on it and the dog-ears `corners`
    /// (left, right) asked for: a page turn shows other pages than the book's current spread.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_book_spread(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        left_i: usize,
        right_i: usize,
        corners: (bool, bool),
        input: &Input,
    ) {
        // The combat book lies on the battlefield; the overland one on a dark ground.
        if self.combat.is_none() {
            let full = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.1, 0.08, 0.06, 1.0),
            );
        }
        if app.draw_pic(lay, "spells", 0, 0, 16.0, 10.0).is_none() {
            let r = lay.rect((16.0, 10.0, 285.0, 186.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.55, 0.4, 0.2, 1.0));
        }
        if corners.0 {
            app.draw_pic(lay, "spells", 1, 0, 29.0, 14.0);
        }
        if corners.1 {
            app.draw_pic(lay, "spells", 2, 0, 274.0, 14.0);
        }
        for (side, page_i) in [(0usize, left_i), (1, right_i)] {
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            self.draw_book_page(
                app,
                lay,
                view,
                page_i,
                side == 1,
                x0,
                (x0 + 54.0, 16.0),
                0.0,
            );
        }
        if self.combat.is_none() {
            let line = app.text(self.filter.text_key());
            let hover = ui::hit(lay.rect((32.0, 184.0, 130.0, 12.0)), true, input).hovered();
            let ink = if hover {
                Color::new(1.0, 0.95, 0.6, 1.0)
            } else {
                Color::new(0.85, 0.72, 0.45, 1.0)
            };
            app.draw_styled(view, &Style::nearest(1, ink).center(), &line, 97.0, 187.0);
            let line = app.text(self.sort.text_key());
            let hover = ui::hit(lay.rect((200.0, 184.0, 116.0, 12.0)), true, input).hovered();
            let ink = if hover {
                Color::new(1.0, 0.95, 0.6, 1.0)
            } else {
                Color::new(0.85, 0.72, 0.45, 1.0)
            };
            app.draw_styled(view, &Style::nearest(1, ink).center(), &line, 258.0, 187.0);
        }
    }

    /// A page of the small book: its title (on the left page and on a right page that starts a
    /// realm) centred at `title`, and its spells with the page's left edge at `x0` and `dy`
    /// added to their y.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_book_page(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        page_i: usize,
        right: bool,
        x0: f32,
        title: (f32, f32),
        dy: f32,
    ) {
        let Some(page) = self.pages.get(page_i).cloned() else {
            return;
        };
        let starts_realm = page_i == 0
            || self
                .pages
                .get(page_i - 1)
                .is_none_or(|p| p.title != page.title);
        if !right || starts_realm {
            app.draw_styled(
                view,
                &Style::nearest(4, Color::new(0.45, 0.2, 0.1, 1.0)).center(),
                &page.title,
                title.0,
                title.1,
            );
        }
        for (i, &spell) in page.spells.iter().enumerate() {
            self.draw_book_entry(app, lay, view, spell, x0, 31.0 + 22.0 * i as f32 + dy);
        }
    }

    /// One spell on a page: name and cost on a line, the turns it takes under them.
    fn draw_book_entry(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        spell: SpellId,
        x: f32,
        y: f32,
    ) {
        let def = app.game.content().rules.spells.get(spell).clone();
        if let Some(entry) = self.combat.as_ref().and_then(|m| m.get(&spell)).copied() {
            app.draw_combat_entry(lay, view, &def.name, &def.realm, entry, x, y);
            return;
        }
        let casting = app
            .game
            .wizard(HUMAN)
            .and_then(|m| m.casting())
            .filter(|c| c.spell == spell);
        let cost = casting.map_or_else(
            || app.game.casting_cost(HUMAN, spell, false),
            |c| c.remaining,
        );
        // Castable on the map; known but only for battle; not learnt (all spells are listed).
        let usable = app.book_spell_usable(spell);
        let known = app.game.wizard(HUMAN).is_some_and(|m| m.knows(spell));
        let ink = if usable {
            Color::new(0.2, 0.12, 0.05, 1.0)
        } else if known {
            Color::new(0.5, 0.38, 0.26, 1.0)
        } else {
            Color::new(0.8, 0.77, 0.72, 1.0)
        };
        let symbol_tint = if known {
            WHITE
        } else {
            Color::new(0.75, 0.75, 0.75, 0.55)
        };
        // The spell being cast glows (palette index 198 cycling, `SmlBook_Draw`).
        let hot = crate::cycle::SMALL_BOOK.colour(get_time());
        let color = if casting.is_some() { hot } else { ink };
        app.draw_styled(view, &Style::nearest(1, color), &def.name, x, y);
        app.draw_styled(
            view,
            &Style::nearest(1, color).right(),
            &format!("{} MP", cost.max(0)),
            x + 121.0,
            y,
        );
        // Turns to cast: a symbol for each, twenty to a row; no symbols for an instant spell.
        let skill = app.game.wizard(HUMAN).map_or(0, |m| m.casting_skill());
        let mana = app.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        let income = app.game.income(HUMAN).split.mana;
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
            let instant = app
                .game
                .wizard(HUMAN)
                .map_or(0, |m| m.skill_left())
                .min(mana)
                >= cost;
            let crafting = matches!(
                def.id.as_str(),
                "enchant_item" | "create_artifact" | "resurrection"
            );
            if instant && !crafting {
                let word = app.text("spell.instant");
                app.draw_styled(view, &Style::nearest(1, ink), &word, x + 9.0, y + 7.0);
            } else if instant {
                let word = app.text("spell.special");
                app.draw_styled(view, &Style::nearest(1, ink), &word, x + 9.0, y + 7.0);
            } else {
                app.draw_pic_tinted(
                    lay,
                    "spells",
                    realm_symbol,
                    0,
                    x + 1.0,
                    y + 7.0,
                    symbol_tint,
                );
            }
        } else {
            for t in 0..turns {
                let (col, row) = (t % 20, t / 20);
                app.draw_pic_tinted(
                    lay,
                    "spells",
                    realm_symbol,
                    0,
                    x + 1.0 + 6.0 * col as f32,
                    y + 7.0 + 6.0 * row as f32,
                    symbol_tint,
                );
            }
        }
    }
}

impl Page for BookUi {
    fn spell_at(&self, x: f32, y: f32) -> Option<(SpellId, (f32, f32, f32, f32))> {
        self.book_spell_at(x, y)
    }

    fn start_test_turn(&mut self, app: &mut App, forward: bool, seconds: f64) {
        Book::start_test_turn(self, app, forward, seconds);
    }

    fn frame(&mut self, app: &mut App, input: &Input) {
        self.frame_in(app, input);
        app.book_prefs = BookPrefs {
            sort: self.sort,
            filter: self.filter,
            last_left: self.last_left,
        };
    }
}

impl Book for BookUi {
    const BIG: bool = false;

    fn left(&self) -> usize {
        self.left
    }

    fn set_left(&mut self, left: usize) {
        self.left = left;
    }

    fn turn(&self) -> Option<BookTurn> {
        self.turn
    }

    fn set_turn(&mut self, turn: Option<BookTurn>) {
        self.turn = turn;
    }

    fn draw_turn_spread(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        left_i: usize,
        right_i: usize,
        input: &Input,
    ) {
        self.draw_book_spread(app, lay, view, left_i, right_i, (left_i != 0, true), input);
    }

    fn draw_turn_leaf(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        page_i: usize,
    ) {
        self.draw_book_page(
            app,
            lay,
            view,
            page_i,
            page_i % 2 == 1,
            0.0,
            (62.0, 30.0),
            17.0,
        );
    }
}

impl BookUi {
    /// One frame of the open book: the questions over it, the page turns, the clicks.
    fn frame_in(&mut self, app: &mut App, input: &Input) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        // A question about a spell the wizard does not know, or why it cannot be learnt.
        if let Some(ask) = self.ask {
            clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
            self.draw_spellbook(app, &lay, view, input);
            self.answer_book_ask(app, ask, input);
            return;
        }
        // A question first: give up the spell being cast for another?
        if let Some((spell, abort_only)) = self.replace {
            clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
            self.draw_spellbook(app, &lay, view, input);
            let casting = app
                .game
                .wizard(HUMAN)
                .and_then(|m| m.casting())
                .map(|c| app.game.content().rules.spells.get(c.spell).name.clone())
                .unwrap_or_default();
            let question = app.text_with("spell.abort", &Args::new().text("spell", &casting));
            match app.confirm_box(&question, input) {
                Some(true) => {
                    self.replace = None;
                    let events = app.game.apply(Command::CancelCast { player: HUMAN });
                    app.note(events);
                    if !abort_only
                        && app
                            .game
                            .wizard(HUMAN)
                            .is_some_and(|m| m.casting().is_none())
                    {
                        app.begin_cast(spell);
                    }
                }
                Some(false) => self.replace = None,
                None => {}
            }
            return;
        }
        if self.turn.is_some() && !app.book_turn_frame(self, input) {
            return;
        }
        let pages = self.pages.len();
        let left = self.left;
        // Page turns: the corners, the arrow keys and the wheel.
        let (_, wheel) = input.mouse_wheel();
        if (input.key_pressed(KeyCode::Right) || input.key_pressed(KeyCode::F) || wheel < 0.0)
            && left + 2 < pages
        {
            app.turn_book_page(self, true, input);
            return;
        }
        if (input.key_pressed(KeyCode::Left) || input.key_pressed(KeyCode::B) || wheel > 0.0)
            && left >= 2
        {
            app.turn_book_page(self, false, input);
            return;
        }
        let click_in =
            |x: f32, y: f32, w: f32, h: f32| ui::hit(lay.rect((x, y, w, h)), true, input).clicked();
        if click_in(29.0, 14.0, 14.0, 13.0) && left >= 2 {
            app.turn_book_page(self, false, input);
            return;
        }
        if click_in(274.0, 14.0, 14.0, 13.0) && left + 2 < pages {
            app.turn_book_page(self, true, input);
            return;
        }
        // The ribbon at the bottom, Escape, V or a click outside the book close it.
        let ribbon = click_in(175.0, 166.0, 19.0, 30.0);
        if ribbon || input.key_pressed(KeyCode::Escape) || input.key_pressed(KeyCode::V) {
            self.last_left = self.left;
            app.screen = Screen::Map;
            return;
        }
        // The sort line at the right of the bookmark (or the S key) changes the order.
        if click_in(200.0, 184.0, 116.0, 12.0) || input.key_pressed(KeyCode::S) {
            self.sort = self.sort.next();
            self.refilter_book(app);
            return;
        }
        // The filter line at the foot of the left page (or the Tab key) changes what is listed.
        if click_in(32.0, 184.0, 130.0, 12.0) || input.key_pressed(KeyCode::Tab) {
            self.filter = self.filter.next();
            self.refilter_book(app);
            return;
        }
        // Clicking a spell.
        let left = self.left;
        for (side, page_i) in [(0usize, left), (1, left + 1)] {
            let Some(page) = self.pages.get(page_i) else {
                continue;
            };
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            for (i, &spell) in page.spells.iter().enumerate() {
                if click_in(x0, 29.0 + 22.0 * i as f32, 121.0, 17.0) {
                    if app.book_spell_usable(spell) {
                        self.last_left = left;
                        self.choose_spell_to_cast(app, spell);
                        return;
                    }
                    if !app.game.wizard(HUMAN).is_some_and(|m| m.knows(spell)) {
                        self.last_left = left;
                        self.ask = Some(app.why_not_learnable(spell));
                        return;
                    }
                    // A known spell that cannot be cast from here (a battle spell on the map)
                    // is described instead.
                    self.last_left = left;
                    app.ask_spell_help(spell, (x0, 29.0 + 22.0 * i as f32, 121.0, 17.0));
                    return;
                }
            }
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_spellbook(app, &lay, view, input);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::BookFilter;

    #[test]
    fn the_filter_cycles_through_the_four_lists_and_starts_on_known_overland() {
        assert_eq!(BookFilter::default(), BookFilter::KnownOverland);
        let mut f = BookFilter::All;
        let seen: Vec<_> = (0..5)
            .map(|_| {
                let now = f;
                f = f.next();
                now
            })
            .collect();
        assert_eq!(
            seen,
            [
                BookFilter::All,
                BookFilter::Known,
                BookFilter::KnownOverland,
                BookFilter::KnownBattle,
                BookFilter::All
            ]
        );
    }
}
